mod trie;

use crate::dictionary;
use crate::filesystem::{store_path, tmp_path};
use anyhow::Context;
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

struct State {
    progress: Option<git2::Progress<'static>>,
    total: usize,
    current: usize,
    path: Option<PathBuf>,
    newline: bool,
}

fn print(state: &mut State) {
    let stats = state.progress.as_ref().unwrap();
    let network_pct = (100 * stats.received_objects()) / stats.total_objects();
    let index_pct = (100 * stats.indexed_objects()) / stats.total_objects();
    let co_pct = if state.total > 0 {
        (100 * state.current) / state.total
    } else {
        0
    };
    let kilobytes = stats.received_bytes() / 1024;
    if stats.received_objects() == stats.total_objects() {
        if !state.newline {
            println!();
            state.newline = true;
        }
        print!(
            "Resolving deltas {}/{}\r",
            stats.indexed_deltas(),
            stats.total_deltas()
        );
    } else {
        print!(
            "net {:3}% ({:4} kb, {:5}/{:5})  /  idx {:3}% ({:5}/{:5})  \
             /  chk {:3}% ({:4}/{:4}) {}\r",
            network_pct,
            kilobytes,
            stats.received_objects(),
            stats.total_objects(),
            index_pct,
            stats.indexed_objects(),
            stats.total_objects(),
            co_pct,
            state.current,
            state.total,
            state
                .path
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    }
    std::io::stdout().flush().unwrap();
}

fn clone<P: AsRef<Path>>(url: &str, path: P) -> Result<git2::Repository, git2::Error> {
    let state = RefCell::new(State {
        progress: None,
        total: 0,
        current: 0,
        path: None,
        newline: false,
    });
    let mut cb = git2::RemoteCallbacks::new();
    cb.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        print(&mut *state);
        true
    });

    let mut co = git2::build::CheckoutBuilder::new();
    co.progress(|path, cur, total| {
        let mut state = state.borrow_mut();
        state.path = path.map(|p| p.to_path_buf());
        state.current = cur;
        state.total = total;
        print(&mut *state);
    });

    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(cb);
    let repo = git2::build::RepoBuilder::new()
        .fetch_options(fo)
        .with_checkout(co)
        .clone(url, path.as_ref())?;
    println!();

    Ok(repo)
}

pub fn import() -> anyhow::Result<()> {
    let repo_path = tmp_path().join("cspell-dicts");
    if !repo_path.exists() {
        fs::create_dir_all(&repo_path).context(format!(
            "Failed to create temporary directory: {}",
            repo_path.display()
        ))?;

        let url = "https://github.com/streetsidesoftware/cspell-dicts";
        println!("Cloning {url}");
        let repo = clone(url, &repo_path).with_context(|| format!("failed to clone: {}", url))?;
    }
    // TODO: checkout right commit (last tag)

    println!("Installing cspell dictionaries");
    let dicts_root = repo_path.join("dictionaries");

    for entry in fs::read_dir(&dicts_root)? {
        let entry = entry?;
        let dict_dir = entry.path();
        let dict_subdir = dict_dir.join("dict");
        if !dict_subdir.exists() {
            continue;
        }

        // collect just the file-names (e.g. "ada.txt"), not full paths
        let mut files = Vec::new();
        for file_entry in fs::read_dir(&dict_subdir)? {
            let file_entry = file_entry?;
            let p = file_entry.path();
            if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                if glob::Pattern::new("*.txt")?.matches(fname) {
                    files.push(fname.to_string());
                }
            }
        }

        let store = store_path().join(format!(
            "cspell_{}",
            dict_dir.file_name().unwrap().to_string_lossy()
        ));
        if store.exists() {
            fs::remove_dir_all(&store)
                .context(format!("Failed to remove directory: {}", store.display()))?;
        }
        fs::create_dir_all(&store)
            .context(format!("Failed to create directory: {}", store.display()))?;

        let mut config = dictionary::DictionaryConfig {
            name: dict_dir.file_name().unwrap().to_string_lossy().into(),
            description: Some("Imported from cspell".to_string()),
            paths: Vec::with_capacity(files.len()),
            case_sensitive: false,
            no_cache: false,
        };

        for file_name in files {
            let src = dict_subdir.join(&file_name);
            let dst = store.join(&file_name);
            fs::copy(&src, &dst).context(format!(
                "Failed to copy file from {} to {}",
                src.display(),
                dst.display(),
            ))?;
            config.paths.push(file_name);
        }
        // Write the config file
        let config_path = store.join("csc-config.json");
        let config_content =
            serde_json::to_string_pretty(&config).context("Failed to serialize config")?;
        let mut config_file = fs::File::create(&config_path).context(format!(
            "Failed to create config file: {}",
            config_path.display()
        ))?;
        config_file
            .write(config_content.as_bytes())
            .context(format!(
                "Failed to write config file: {}",
                config_path.display()
            ))?;

        println!("Installed dictionary: {}", config.name);
    }
    Ok(())
}
