use std::{io::BufRead, path::PathBuf};

use ahash::HashMapExt;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::{HashMap, Trie, filesystem, store_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    CaseSensitive,
    Cache(bool),
}

impl Command {
    pub fn from_str(s: &str) -> Option<Self> {
        if s == "case-sensitive" {
            Some(Self::CaseSensitive)
        } else if s.starts_with("cache:") {
            let value = s.trim_start_matches("cache:");
            if value == "true" {
                Some(Self::Cache(true))
            } else if value == "false" {
                Some(Self::Cache(false))
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Rule {
    /// A rule that allows a word
    Allow(String),
    /// A rule that disallows a word
    Disallow(String),
    /// A command rule
    Command(Command),
    /// A comment
    Comment(String),
}

fn load_dictionary_line(line: &str) -> anyhow::Result<Rule> {
    // let trimmed = line.trim();
    // TODO: Special for cspell
    let trimmed = line.split("/").next().unwrap_or(line).trim();
    if trimmed.is_empty() {
        return Ok(Rule::Comment("".to_string())); // Empty lines are ignored
    }
    Ok(if trimmed.starts_with('#') || trimmed.starts_with("//") {
        let comment = trimmed
            .trim_start_matches('#')
            .trim_start_matches("//")
            .trim()
            .to_string();
        if comment.starts_with("csc:") {
            let command = comment.trim_start_matches("csc:").trim();
            if let Some(cmd) = Command::from_str(command) {
                Rule::Command(cmd)
            } else {
                Rule::Comment(comment)
            }
        } else {
            Rule::Comment(comment)
        }
        // TODO: Handle case sensitivity
    } else if trimmed.starts_with("!") {
        let disallow = trimmed.trim_start_matches('!').trim().to_ascii_lowercase();
        Rule::Disallow(disallow)
    } else if trimmed.starts_with("+") {
        let allow = trimmed.trim_start_matches('+').trim().to_ascii_lowercase();
        Rule::Allow(allow)
    } else {
        Rule::Allow(trimmed.to_ascii_lowercase().to_string())
    })
}

fn load_dictionary_format(s: &str) -> anyhow::Result<Vec<Rule>> {
    s.lines()
        .map(load_dictionary_line)
        .collect::<Result<Vec<_>, _>>()
}

fn load_dictionary_format_from_file<P: AsRef<std::path::Path>>(p: P) -> anyhow::Result<Vec<Rule>> {
    let file = std::fs::File::open(p)?;
    // stream lines for memory efficiency
    let reader = std::io::BufReader::new(file);
    let mut rules = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let rule = load_dictionary_line(&line)?;
        rules.push(rule);
    }
    Ok(rules)
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct DictCacheStore(pub HashMap<String, String>);

impl DictCacheStore {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let data = std::fs::read(path);
        if data.is_err() {
            return Ok(Self::new());
        }
        let data = data?;
        let store: Self = serde_hjson::from_slice(&data).unwrap_or_default();

        Ok(store)
    }

    pub fn dump_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let data = serde_json::to_vec(self).expect("Failed to serialize TrieHashStore");
        std::fs::write(path, data)
    }
}

pub fn dict_cache_store_location() -> anyhow::Result<PathBuf> {
    let mut path = crate::cache_path();
    path.push("cache.json");
    Ok(path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub no_cache: bool,
    #[serde(default)]
    pub globs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dictionary {
    /// A dictionary that is loaded from a file
    File(PathBuf),
    /// A dictionary that is loaded from a directory
    Directory(PathBuf),
    /// A cspell trie
    Trie(PathBuf),
    /// Custom
    Custom {
        definition: crate::settings::CustomDictionaryDefinition,
        root: PathBuf,
    },
    /// A dictionary that is loaded from a vector of rules
    Rules(Vec<Rule>),
}

impl Dictionary {
    pub fn new_with_path(path: PathBuf) -> anyhow::Result<Self> {
        let mut path = path;
        // If path is relative check if it exists in store_path
        let store_path = store_path();
        if !path.exists() && path.is_relative() && store_path.join(&path).exists() {
            path = store_path.join(&path);
        }
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Dictionary path does not exist: {}",
                path.display()
            ));
        }
        if path.is_dir() {
            Ok(Self::Directory(path))
        } else if path.is_file() {
            Ok(Self::File(path))
        } else {
            Err(anyhow::anyhow!(
                "Invalid dictionary path: {}",
                path.display()
            ))
        }
    }

    pub fn new_custom(
        definition: crate::settings::CustomDictionaryDefinition,
        root: PathBuf,
    ) -> Self {
        Self::Custom { definition, root }
    }

    pub fn new_with_rules(rules: Vec<Rule>) -> Self {
        Self::Rules(rules)
    }

    pub fn new_from_strings(strings: &[String]) -> Self {
        let rules = strings
            .iter()
            .map(|s| load_dictionary_line(s))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        Self::Rules(rules)
    }

    fn load_from_cache_inner(&self, path: &PathBuf) -> anyhow::Result<Option<Trie>> {
        let path_hash = blake3::hash(path.to_str().unwrap().as_bytes())
            .to_hex()
            .to_string();
        let fs_hash = filesystem::get_path_hash(path)?;
        let cache_hash_store = DictCacheStore::load_from_file(dict_cache_store_location()?)?;
        if let Some(hash) = cache_hash_store.0.get(&path_hash) {
            if hash == &fs_hash {
                let cache_path = filesystem::cache_path().join(format!("{path_hash}.bin"));
                if cache_path.exists() {
                    let trie = Trie::load_from_file(cache_path)?;
                    return Ok(Some(trie));
                }
            }
        }
        Ok(None)
    }

    pub fn load_from_cache(&self, path: &PathBuf) -> anyhow::Result<Option<Trie>> {
        self.load_from_cache_inner(path)
            .context(format!("Failed to load cache for {}", path.display()))
    }

    fn save_to_cache_inner(trie: &Trie, path: &PathBuf) -> anyhow::Result<()> {
        let path_hash = blake3::hash(path.to_str().unwrap().as_bytes())
            .to_hex()
            .to_string();
        let fs_hash = filesystem::get_path_hash(path)?;
        let cache_path = filesystem::cache_path().join(format!("{path_hash}.bin"));
        trie.dump_to_file(&cache_path)?;
        let mut cache_hash_store = DictCacheStore::load_from_file(dict_cache_store_location()?)?;
        cache_hash_store.0.insert(path_hash, fs_hash);
        cache_hash_store.dump_to_file(dict_cache_store_location()?)?;
        Ok(())
    }

    pub fn save_to_cache(trie: &Trie, path: &PathBuf) -> anyhow::Result<()> {
        Self::save_to_cache_inner(trie, path)
            .context(format!("Failed to save cache for {}", path.display()))
    }

    pub fn get_names(&self) -> anyhow::Result<Vec<String>> {
        match self {
            Self::File(path) | Self::Trie(path) => Ok(vec![
                path.file_stem().unwrap().to_string_lossy().to_string(),
            ]),
            Self::Custom { definition, .. } => Ok(vec![definition.name.clone()]),
            Self::Directory(path) => {
                let config_path = path.join("csc-config.json");
                if !config_path.exists() {
                    return Err(anyhow::anyhow!(
                        "Dictionary config file does not exist: {}",
                        config_path.display()
                    ));
                }
                let content: DictionaryConfig =
                    serde_hjson::from_reader(std::fs::File::open(config_path)?)?;
                Ok(vec![content.name])
            }
            Self::Rules(_) => Ok(vec![]),
        }
    }

    pub fn get_globs(&self) -> anyhow::Result<Option<Vec<glob::Pattern>>> {
        match self {
            Self::File(path) => {
                let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                let pattern = glob::Pattern::new(&file_name)?;
                Ok(Some(vec![pattern]))
            }
            Self::Directory(path) => {
                let config_path = path.join("csc-config.json");
                if !config_path.exists() {
                    return Err(anyhow::anyhow!(
                        "Dictionary config file does not exist: {}",
                        config_path.display()
                    ));
                }
                let content: DictionaryConfig =
                    serde_hjson::from_reader(std::fs::File::open(config_path)?)?;
                if content.globs.len() > 0 {
                    let mut patterns = Vec::new();
                    for glob in &content.globs {
                        let pattern = glob::Pattern::new(glob)?;
                        patterns.push(pattern);
                    }
                    Ok(Some(patterns))
                } else {
                    Ok(None)
                }
            }
            Self::Custom { definition, .. } => {
                if definition.globs.len() > 0 {
                    let mut patterns = Vec::new();
                    for glob in &definition.globs {
                        let pattern = glob::Pattern::new(glob)?;
                        patterns.push(pattern);
                    }
                    Ok(Some(patterns))
                } else {
                    Ok(None)
                }
            }
            Self::Rules(_) | Self::Trie(_) => Ok(None),
        }
    }

    fn compile_inner(&self) -> anyhow::Result<Trie> {
        match self {
            Self::File(path) => {
                if let Some(cache) = self.load_from_cache(path)? {
                    return Ok(cache);
                }
            }
            Self::Directory(path) => {
                let config_path = path.join("csc-config.json");
                if !config_path.exists() {
                    return Err(anyhow::anyhow!(
                        "Dictionary config file does not exist: {}",
                        config_path.display()
                    ));
                }
                let content: DictionaryConfig =
                    serde_hjson::from_reader(std::fs::File::open(config_path)?)?;
                if !content.no_cache {
                    if let Some(cache) = self.load_from_cache(path)? {
                        return Ok(cache);
                    }
                }
            }
            Self::Rules(_) | Self::Custom { .. } => {}
            Self::Trie(path) => {
                if let Some(cache) = self.load_from_cache(path)? {
                    return Ok(cache);
                }
            }
        }
        match self {
            Self::File(path) => {
                let rules = load_dictionary_format_from_file(path)?;
                let trie = Trie::from(rules.as_ref());
                if trie.options.cache {
                    Self::save_to_cache(&trie, path)?;
                }
                Ok(trie)
            }
            Self::Custom { definition, root } => {
                let mut rules = vec![];
                let path = root.join(definition.path());
                if !path.exists() {
                    return Err(anyhow::anyhow!(
                        "Custom dictionary file does not exist: {}",
                        path.display()
                    ));
                }
                let rules_part = load_dictionary_format_from_file(&path)?;
                rules.extend(rules_part);
                Ok(Trie::from(rules.as_ref()))
            }
            Self::Directory(path) => {
                let config_path = path.join("csc-config.json");
                if !config_path.exists() {
                    return Err(anyhow::anyhow!(
                        "Dictionary config file does not exist: {}",
                        config_path.display()
                    ));
                }
                let content: DictionaryConfig =
                    serde_hjson::from_reader(std::fs::File::open(config_path)?)?;
                let mut rules = Vec::new();
                for path_str in &content.paths {
                    let path_str = path_str.trim().to_string();
                    let file_path = relative_path::RelativePath::new(&path_str);
                    let file_path = file_path.to_path(path);
                    if file_path.exists() {
                        if file_path.extension().unwrap().to_str().unwrap() == "trie" {
                            let mut trie = crate::cspell::CspellTrie::parse_trie(&file_path)?;
                            if content.paths.len() != 1 {
                                bail!("If trie is compiled, there can only be one path");
                            }
                            trie.options.case_sensitive = content.case_sensitive;
                            trie.options.cache = !content.no_cache;
                            if trie.options.cache {
                                Self::save_to_cache(&trie, path)?;
                            }
                            return Ok(trie);
                        }
                        let rules_part = load_dictionary_format_from_file(&file_path)?;
                        rules.extend(rules_part);
                    } else {
                        return Err(anyhow::anyhow!(
                            "Dictionary file does not exist: {}",
                            path_str
                        ));
                    }
                }
                if content.case_sensitive {
                    rules.push(Rule::Command(Command::CaseSensitive));
                }
                if content.no_cache {
                    rules.push(Rule::Command(Command::Cache(false)));
                } else {
                    rules.push(Rule::Command(Command::Cache(true)));
                }
                let trie = Trie::from(rules.as_ref());
                if trie.options.cache {
                    Self::save_to_cache(&trie, path)?;
                }
                Ok(trie)
            }
            Self::Rules(rules) => {
                let mut new_rules = vec![];
                for rule in rules {
                    new_rules.push(rule.clone());
                }
                new_rules.push(Rule::Command(Command::Cache(false)));
                let trie = Trie::from(new_rules.as_ref());
                Ok(trie)
            }
            Self::Trie(path) => {
                let content = std::fs::read(path)?;
                let trie = Trie::load(&content)?;
                if trie.options.cache {
                    Self::save_to_cache(&trie, path)?;
                }
                Ok(trie)
            }
        }
    }

    pub fn compile(&self) -> anyhow::Result<Trie> {
        self.compile_inner().context("Failed to compile dictionary")
    }
}
