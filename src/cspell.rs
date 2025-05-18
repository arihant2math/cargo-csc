mod trie;

use std::{fs, io::Write};

use anyhow::Context;
use git2::Repository;
pub use trie::CspellTrie;

use crate::{
    dictionary,
    filesystem::{cspell_path, store_path},
};

const URL: &str = "https://github.com/arihant2math/cspell-dicts";

pub fn import() -> anyhow::Result<()> {
    let repo_path = cspell_path().join("cspell-dicts");
    if !repo_path.exists() {
        fs::create_dir_all(&repo_path).context(format!(
            "Failed to create temporary directory: {}",
            repo_path.display()
        ))?;

        println!("Cloning {URL}");
        crate::git::clone(URL, &repo_path).with_context(|| format!("failed to clone: {URL}"))?;
    } else {
        let res = Repository::open(&repo_path);
        match res {
            Ok(repo) => {
                // Update repo
                let mut remote = repo.find_remote("origin")?;
                let remote_branch = "main";
                let fetch_commit = crate::git::fetch(&repo, &[remote_branch], &mut remote)?;
                crate::git::merge(&repo, remote_branch, fetch_commit)?;
                drop(remote);
            }
            Err(e) => {
                eprintln!("Failed to open temporary directory: {e}");
                // Reclone
                fs::remove_dir_all(&repo_path).ok();
                println!("Recloning {URL}");
                crate::git::clone(URL, &repo_path)
                    .with_context(|| format!("failed to clone: {URL}"))?;
            }
        }
    }

    println!("Installing cspell dictionaries");
    let dicts_root = repo_path.join("dictionaries");

    for entry in fs::read_dir(&dicts_root)? {
        let entry = entry?;
        let dict_dir = entry.path();
        let dict_subdir = dict_dir.join("dict");

        // collect just the file-names (e.g. "ada.txt"), not full paths
        let mut files = Vec::new();
        if dict_subdir.exists() {
            for file_entry in fs::read_dir(&dict_subdir)? {
                let file_entry = file_entry?;
                let p = file_entry.path();
                if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                    if glob::Pattern::new("*.txt")?.matches(fname) {
                        files.push(p.canonicalize()?);
                    }
                }
            }
        }
        for file_entry in fs::read_dir(&dict_dir)? {
            let file_entry = file_entry?;
            let p = file_entry.path();
            if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                if glob::Pattern::new("*.trie")?.matches(fname) {
                    files.push(p.canonicalize()?);
                }
            }
        }
        if files.is_empty() {
            continue;
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

        for src in files {
            let file_name = src
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Failed to get file name"))?
                .to_string_lossy()
                .into_owned();
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
