use anyhow::{anyhow, Context};
use git2::Repository;
use std::path::{Path, PathBuf};
use std::fs;
use tokio::task::JoinSet;
use crate::filesystem::{repo_path, store_path};

pub async fn copy_tree<U: AsRef<Path>, V: AsRef<Path>>(from: U, to: V) -> Result<(), std::io::Error> {
    let mut stack = vec![PathBuf::from(from.as_ref())];

    let output_root = PathBuf::from(to.as_ref());
    let input_root = PathBuf::from(from.as_ref()).components().count();

    let mut join_set = JoinSet::new();
    while let Some(working_path) = stack.pop() {
        // Generate a relative path
        let src: PathBuf = working_path.components().skip(input_root).collect();

        // Create a destination if missing
        let dest = if src.components().count() == 0 {
            output_root.clone()
        } else {
            output_root.join(&src)
        };
        if fs::metadata(&dest).is_err() {
            tokio::fs::create_dir_all(&dest).await?;
        }

        for entry in fs::read_dir(working_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                match path.file_name() {
                    Some(filename) => {
                        let dest_path = dest.join(filename);
                        join_set.spawn(async {fs::copy(path, dest_path)});
                    }
                    None => {
                        println!("failed: {:?}", path);
                    }
                }
            }
        }
    }
    for r in join_set.join_all().await {
        r?;
    }

    Ok(())
}

const URL: &str = "https://github.com/arihant2math/cargo-csc-dicts.git";

pub async fn import() -> anyhow::Result<()> {
    let repo_path = repo_path().join("cargo-csc-dicts");
    if !repo_path.exists() {
        tokio::fs::create_dir_all(&repo_path)
            .await
            .context(format!(
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
                tokio::fs::remove_dir_all(&repo_path).await?;
                println!("Recloning {URL}");
                crate::git::clone(URL, &repo_path)
                    .with_context(|| format!("failed to clone: {URL}"))?;
            }
        }
    }

    println!("Installing first-party dictionaries");
    // copy the whole tree from repo_path/dicts to store_path
    let dicts_path = repo_path.join("dicts");
    if !dicts_path.exists() {
        return Err(anyhow!("Failed to find dicts directory in cargo-csc repo"));
    }
    copy_tree(&dicts_path, store_path().join("first_party")).await?;
    Ok(())
}

pub async fn unimport_cspell() -> anyhow::Result<()> {
    let repo_path = repo_path().parent().unwrap().join("cspell").join("cspell-dicts");
    if repo_path.exists() {
        tokio::fs::remove_dir_all(&repo_path)
            .await
            .context(format!(
                "Failed to remove temporary directory: {}",
                repo_path.display()
            ))?;
        println!("Removed cspell repo");
    } else {
        println!("No cspell repo to remove");
    }
    let store = store_path();
    for entry in fs::read_dir(&store)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(fname) = path.file_name().and_then(|s| s.to_str())
            && fname.starts_with("cspell_")
        {
            tokio::fs::remove_dir_all(&path)
                .await
                .context(format!("Failed to remove directory: {}", path.display()))?;
            println!("Removed dictionary: {}", fname);
        }
    }
    Ok(())
}
