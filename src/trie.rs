use std::collections::HashMap;

use bincode::{Decode, Encode};

use crate::dictionary::{Command, Rule};

#[derive(Clone, Debug, Encode, Decode)]
pub struct TrieData {
    pub disallow: bool,
}

impl Default for TrieData {
    fn default() -> Self {
        TrieData { disallow: false }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode)]
pub struct TrieNode {
    pub data: Option<TrieData>,
    pub children: HashMap<char, TrieNode>,
}

impl TrieNode {
    pub fn none() -> Self {
        TrieNode {
            data: None,
            children: HashMap::new(),
        }
    }

    pub fn some_default() -> Self {
        TrieNode {
            data: Some(TrieData::default()),
            children: HashMap::new(),
        }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode)]
pub struct Trie {
    pub root: TrieNode,
    pub options: TrieOptions,
}

impl Trie {
    pub fn new() -> Self {
        Trie {
            root: TrieNode::default(),
            options: TrieOptions::new(),
        }
    }

    pub(crate) fn set_root(&mut self, root: TrieNode) {
        self.root = root;
    }

    pub fn dump(&self) -> anyhow::Result<Vec<u8>> {
        Ok(bincode::encode_to_vec(self, bincode::config::standard())?)
    }

    pub fn load(data: &[u8]) -> anyhow::Result<Self> {
        Ok(bincode::decode_from_slice(data, bincode::config::standard()).map(|(trie, _)| trie)?)
    }

    pub fn dump_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let data = self.dump()?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        Trie::load(&data)
    }

    /// Inserts a word into the trie.
    /// If the word already exists, it will be replaced.
    pub fn insert(&mut self, word: &str, data: TrieData) {
        let mut current_node = &mut self.root;

        for c in word.chars() {
            current_node = current_node.children.entry(c).or_default();
        }
        current_node.data = Some(data);
    }

    pub fn contains(&self, word: &str) -> bool {
        let mut current_node = &self.root;

        for c in word.chars() {
            // TODO: handle case sensitivity
            match current_node.children.get(&c) {
                Some(node) => current_node = node,
                None => return false,
            }
        }

        if let Some(ref data) = current_node.data {
            // TODO: handle disallow properly
            !data.disallow
        } else {
            false
        }
    }

    fn collect_words(&self, node: &TrieNode, prefix: String, words: &mut Vec<String>) {
        if let Some(ref data) = node.data {
            if !data.disallow {
                words.push(prefix.clone());
            }
        }

        for (c, child_node) in &node.children {
            let mut new_prefix = prefix.clone();
            new_prefix.push(*c);
            self.collect_words(child_node, new_prefix, words);
        }
    }

    pub fn to_vec(&self) -> Vec<String> {
        let mut words = Vec::new();
        self.collect_words(&self.root, String::new(), &mut words);
        words
    }
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct TrieOptions {
    pub cache: bool,
    pub case_sensitive: bool,
}

impl Default for TrieOptions {
    fn default() -> Self {
        TrieOptions {
            cache: true,
            case_sensitive: false,
        }
    }
}

impl TrieOptions {
    pub fn new() -> Self {
        TrieOptions::default()
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
        let mut trie = Trie::new();
        for rule in rules {
            match rule {
                Rule::Allow(word) => {
                    trie.insert(&word, TrieData::default());
                }
                Rule::Disallow(word) => {
                    trie.insert(&word, TrieData { disallow: true });
                }
                Rule::Command(command) => {
                    trie.options.add_command(command);
                }
                _ => {}
            }
        }
        trie
    }
}
