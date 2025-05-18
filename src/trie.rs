use std::fmt::Debug;

use bincode::{Decode, Encode};
use fst::{IntoStreamer, automaton::Levenshtein};

use crate::dictionary::{Command, Rule};

#[derive(Clone, Encode, Decode)]
struct TrieRepr {
    trie: Vec<u8>,
    options: TrieOptions,
}

#[derive(Clone)]
pub struct Trie {
    pub root: fst::map::Map<Vec<u8>>,
    pub options: TrieOptions,
}

impl Debug for Trie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trie")
            .field("root", &"Elided")
            .field("options", &self.options)
            .finish()
    }
}

impl Default for Trie {
    fn default() -> Self {
        Self {
            root: fst::map::Map::from_iter::<&str, Vec<(&str, u64)>>(vec![]).unwrap(),
            options: TrieOptions::default(),
        }
    }
}

impl Trie {
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: TrieOptions::new(),
            ..Default::default()
        }
    }

    pub fn dump(&self) -> anyhow::Result<Vec<u8>> {
        let trie_repr = TrieRepr {
            trie: self.root.clone().into_fst().to_vec(),
            options: self.options.clone(),
        };
        Ok(bincode::encode_to_vec(
            trie_repr,
            bincode::config::standard(),
        )?)
    }

    pub fn load(data: &[u8]) -> anyhow::Result<Self> {
        let (trie_repr, _): (TrieRepr, _) =
            bincode::decode_from_slice(data, bincode::config::standard())?;
        let root = fst::map::Map::new(trie_repr.trie)?;
        Ok(Self {
            root,
            options: trie_repr.options,
        })
    }

    pub fn dump_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let data = self.dump()?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        Self::load(&data)
    }

    #[must_use]
    pub fn contains(&self, word: &str) -> bool {
        self.root.contains_key(word)
    }

    pub fn to_vec(&self) -> Vec<String> {
        self.root.stream().into_str_keys().unwrap()
    }

    pub fn check(&self, word: &str) -> anyhow::Result<Option<String>> {
        let lev = Levenshtein::new(word, 1)?;
        let stream = self.root.search(lev).into_stream();
        let mut keys = stream.into_str_keys()?;
        keys.sort_by(|s, t| {
            let score1 = strsim::normalized_damerau_levenshtein(word, s);
            let score2 = strsim::normalized_damerau_levenshtein(word, t);
            score1.total_cmp(&score2)
        });
        Ok(keys.last().cloned())
    }
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct TrieOptions {
    pub cache: bool,
    pub case_sensitive: bool,
}

impl Default for TrieOptions {
    fn default() -> Self {
        Self {
            cache: true,
            case_sensitive: false,
        }
    }
}

impl TrieOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_command(&mut self, command: &Command) {
        match command {
            Command::CaseSensitive => self.case_sensitive = true,
            Command::Cache(cache) => self.cache = *cache,
        }
    }
}

impl From<&[Rule]> for Trie {
    fn from(rules: &[Rule]) -> Self {
        let mut trie = Vec::new();
        let mut options = TrieOptions::default();
        for rule in rules {
            match rule {
                Rule::Allow(word) => {
                    trie.push((word, 0));
                }
                Rule::Disallow(word) => {
                    trie.push((word, 1));
                }
                Rule::Command(command) => {
                    options.add_command(command);
                }
                Rule::Comment(_) => {}
            }
        }
        trie.sort_by_key(|(word, _)| word.to_string());
        trie.dedup();
        Self {
            root: fst::map::Map::from_iter(trie).unwrap(),
            options,
        }
    }
}
