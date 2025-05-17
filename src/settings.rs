use std::{fs, path::PathBuf};

use anyhow::Context;
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::filesystem::git_path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomDictionaryDefinitionPath {
    Simple(String),
}

impl CustomDictionaryDefinitionPath {
    pub fn path(&self) -> PathBuf {
        match self {
            Self::Simple(path) => PathBuf::from(path),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomDictionaryDefinitionGitIdent {
    #[serde(rename = "branch")]
    Branch(String),
    #[serde(rename = "tag")]
    Tag(String),
    #[serde(rename = "commit")]
    Commit(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomDictionaryDefinitionGit {
    Simple(String),
    Custom {
        url: String,
        identity: CustomDictionaryDefinitionGitIdent,
    },
}

impl CustomDictionaryDefinitionGit {
    pub fn init(&self) -> anyhow::Result<()> {
        let url = self.url();
        let repo_path = self.path();
        let repo = if !repo_path.exists() {
            fs::create_dir_all(&repo_path).context(format!(
                "Failed to create temporary directory: {}",
                repo_path.display()
            ))?;

            println!("Cloning {url}");
            crate::git::clone(&url, &repo_path)
                .with_context(|| format!("failed to clone: {}", url))?
        } else {
            let res = Repository::open(&repo_path);
            match res {
                Ok(repo) => {
                    // TODO: choose when to update repo
                    let repo_path_info = fs::metadata(&repo_path)?;
                    let secs_since_last_accessed = repo_path_info.accessed()?.elapsed()?.as_secs();

                    const SECONDS_IN_HOUR: u64 = 60 * 60;
                    let should_update = secs_since_last_accessed > SECONDS_IN_HOUR * 3;

                    if should_update {
                        let mut remote = repo.find_remote("origin")?;
                        let remote_branch = "main";
                        let fetch_commit = crate::git::fetch(&repo, &[remote_branch], &mut remote)?;
                        crate::git::merge(&repo, &remote_branch, fetch_commit)?;
                        drop(remote);
                    }
                    repo
                }
                Err(e) => {
                    eprintln!("Failed to open temporary directory: {}", e);
                    // Reclone
                    fs::remove_dir_all(&repo_path).ok();
                    println!("Recloning {url}");
                    crate::git::clone(&url, &repo_path)
                        .with_context(|| format!("failed to clone: {}", url))?
                }
            }
        };
        // TODO: ensure the repo is in a clean state and on the correct identifier
        Ok(())
    }

    pub fn url(&self) -> String {
        match self {
            Self::Simple(url) => url.clone(),
            Self::Custom { url, .. } => url.clone(),
        }
    }

    pub fn path(&self) -> PathBuf {
        let url = self.url();
        let hash = blake3::hash(url.as_bytes());
        let hash_hex = hash.to_hex().to_string();
        git_path().join(hash_hex)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CustomDictionaryDefinitionType {
    #[serde(rename = "path")]
    Path(CustomDictionaryDefinitionPath),
    #[serde(rename = "git")]
    Git(CustomDictionaryDefinitionGit),
}

impl CustomDictionaryDefinitionType {
    pub fn path(&self) -> PathBuf {
        match self {
            Self::Path(path) => path.path(),
            Self::Git(git) => git.path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDictionaryDefinition {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(flatten)]
    pub typ: CustomDictionaryDefinitionType,
}

impl CustomDictionaryDefinition {
    pub fn path(&self) -> PathBuf {
        self.typ.path()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DictionaryName {
    Simple(String),
    Detailed {
        name: String,
        #[serde(default)]
        globs: Vec<String>,
    },
}

impl DictionaryName {
    pub fn name(&self) -> String {
        match self {
            DictionaryName::Simple(name) => name.clone(),
            DictionaryName::Detailed { name, .. } => name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub dictionaries: Vec<DictionaryName>,
    #[serde(default, alias = "dictionaryDefinitions")]
    pub dictionary_definitions: Vec<CustomDictionaryDefinition>,
    #[serde(default, alias = "ignorePaths")]
    pub ignore_paths: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dictionaries: vec![
                DictionaryName::Simple("extra".to_string()),
                DictionaryName::Simple("en-US".to_string()),
                DictionaryName::Simple("software_terms".to_string()),
                DictionaryName::Simple("software_tools".to_string()),
                DictionaryName::Simple("words".to_string()),
            ],
            dictionary_definitions: vec![],
            ignore_paths: vec![],
            words: vec![],
        }
    }
}

impl Settings {
    pub fn new() -> Self {
        Settings::default()
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let settings: Settings = serde_hjson::from_str(&data)?;
        Ok(settings)
    }

    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load(override_: Option<String>) -> Self {
        let path = override_.unwrap_or_else(|| "code-spellcheck.json".to_string());
        if std::path::Path::new(&path).exists() {
            match Settings::load_from_file(&path) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!("Error loading settings from {}: {}", path, e);
                    Settings::default()
                }
            }
        } else {
            Settings::default()
        }
    }
}
