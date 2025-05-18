use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use std::ffi::OsStr;
use anyhow::{Context, bail};

pub fn get_file_extension(file: &Path) -> Option<String> {
    file.extension()
        .and_then(OsStr::to_str)
        .map(ToString::to_string)
}

pub fn csc_path() -> PathBuf {
    let mut path = std::env::home_dir().expect("Failed to get home directory");
    path.push(".code-spellcheck");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create .code-spellcheck directory");
    }
    path
}

macro_rules! subpath {
    ($name: ident, $path: expr) => {
        #[cached::proc_macro::cached(size = 1)]
        #[allow(unused)]
        pub fn $name() -> PathBuf {
            let path = csc_path().join($path);
            if !path.exists() {
                fs::create_dir_all(&path).expect("Failed to create $name directory");
            }
            path
        }
    };
}

subpath!(store_path, "wordlists");
subpath!(cache_path, "cache");
subpath!(tmp_path, "tmp");
subpath!(cspell_path, "custom-dicts/cspell");
subpath!(download_path, "custom-dicts/download");
subpath!(git_path, "custom-dicts/git");

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
        // walk over all files in the directory recursively
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.context("Failed to read directory entry")?;
            if entry.file_type().is_file() {
                let file_path = entry.path();
                let mut file_hasher = blake3::Hasher::new();
                let file = fs::File::open(file_path).context("Failed to open file")?;
                let mut reader = std::io::BufReader::new(file);
                let mut buffer = [0; 8192];
                loop {
                    let bytes_read = reader.read(&mut buffer)?;
                    if bytes_read == 0 {
                        break;
                    }
                    file_hasher.update(&buffer[..bytes_read]);
                }
                hasher.update(file_hasher.finalize().as_bytes());
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}
