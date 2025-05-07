use anyhow::{Context, bail};

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub fn get_file_extension(file: &Path) -> Option<String> {
    file.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string())
}

pub fn csc_path() -> PathBuf {
    #[allow(deprecated)]
    let mut path = std::env::home_dir().expect("Failed to get home directory");
    path.push(".code-spellcheck");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create .code-spellcheck directory");
    }
    path
}

pub fn store_path() -> PathBuf {
    let mut path = csc_path();
    path.push("wordlists");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create wordlists directory");
    }
    path
}

pub fn cache_path() -> PathBuf {
    let mut path = csc_path();
    path.push("caches");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create wordlists directory");
    }
    path
}

pub fn tmp_path() -> PathBuf {
    let mut path = csc_path();
    path.push("tmp");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create wordlists directory");
    }
    path
}

pub fn get_path_hash<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    if !path.as_ref().exists() {
        bail!("Path does not exist: {}", path.as_ref().display());
    }
    let path = path.as_ref();
    let mut hasher = blake3::Hasher::new();
    if path.is_file() {
        let file = fs::File::open(path).context("Failed to open file")?;
        let mut reader = std::io::BufReader::new(file);
        let mut buffer = [0; 8192];
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
    } else if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                hasher.update(path.to_str().unwrap().as_bytes());
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}
