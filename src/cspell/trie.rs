// mod v4;

mod spec;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::Trie;

trait CspellTrieVersion {
    fn read(lines: &[String]) -> anyhow::Result<Trie>;

    fn write(trie: &crate::Trie) -> anyhow::Result<Vec<String>>;
}

struct V3;

impl CspellTrieVersion for V3 {
    fn read(lines: &[String]) -> anyhow::Result<Trie> {
        let res = spec::parse_trie(lines)?;
        Ok(res.1)
    }

    fn write(_trie: &Trie) -> anyhow::Result<Vec<String>> {
        todo!()
    }
}

struct V4;

impl CspellTrieVersion for V4 {
    fn read(lines: &[String]) -> anyhow::Result<Trie> {
        let res = spec::parse_trie(lines)?;
        Ok(res.1)
    }

    fn write(_trie: &Trie) -> anyhow::Result<Vec<String>> {
        todo!()
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialOrd,
    PartialEq,
    Ord,
    Eq,
    Serialize,
    Deserialize,
    Encode,
    Decode,
)]
pub struct CspellTrie;

impl CspellTrie {
    pub fn parse_trie<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Trie> {
        let converted = spec::file_to_lines(path)?;
        let (_, trie) = spec::parse_trie(converted.as_slice())?;
        Ok(trie)
    }
}
