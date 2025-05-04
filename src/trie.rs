use core::str;
use std::collections::HashMap;
use std::io::BufRead;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Encode, Decode)]
struct TrieNode {
    is_end_of_word: bool,
    children: HashMap<char, TrieNode>,
}

#[derive(Default, Debug, Encode, Decode)]
pub struct Trie {
    root: TrieNode,
}

impl Trie {
    pub fn new() -> Self {
        Trie {
            root: TrieNode::default(),
        }
    }

    pub fn from_iterator<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut trie = Trie::new();
        for word in iter {
            trie.insert(&word.to_ascii_lowercase());
        }
        trie
    }

    pub fn append_from_iterator<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = String>,
    {
        for word in iter {
            self.insert(&word.to_ascii_lowercase());
        }
    }

    pub fn dump(&self) -> Vec<u8> {
        bincode::encode_to_vec(&self, bincode::config::standard()).unwrap()
    }

    pub fn load(data: &[u8]) -> Self {
        bincode::decode_from_slice(data, bincode::config::standard())
            .map(|(trie, _)| trie)
            .unwrap()
    }

    pub fn dump_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let data = self.dump();
        std::fs::write(path, data)
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        Ok(Trie::load(&data))
    }

    pub fn from_wordlist<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut trie = Trie::new();
        for line in reader.lines() {
            let line = line?;
            let stripped_line = line.trim();
            // Ignore empty lines and comments
            if stripped_line.starts_with('#') || stripped_line.starts_with("//") {
                continue;
            }
            if !stripped_line.is_empty() {
                trie.insert(&stripped_line.to_ascii_lowercase());
            }
        }
        Ok(trie)
    }

    fn from_directory<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        // code-spellcheck.json must exist
        let path = path.as_ref().join("code-spellcheck.json");
    }

    pub fn insert(&mut self, word: &str) {
        let mut current_node = &mut self.root;

        for c in word.chars() {
            current_node = current_node.children.entry(c).or_default();
        }
        current_node.is_end_of_word = true;
    }

    pub fn contains(&self, word: &str) -> bool {
        let mut current_node = &self.root;

        for c in word.chars() {
            match current_node.children.get(&c) {
                Some(node) => current_node = node,
                None => return false,
            }
        }

        current_node.is_end_of_word
    }
}

// TODO: finish implementing this
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct TrieHashStore(pub HashMap<String, String>);

impl TrieHashStore {
    pub fn new() -> Self {
        TrieHashStore(HashMap::new())
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        let store: TrieHashStore = serde_json::from_slice(&data).unwrap_or_default();

        Ok(store)
    }

    pub fn dump_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let data = serde_json::to_vec(self).expect("Failed to serialize TrieHashStore");
        std::fs::write(path, data)
    }
}
