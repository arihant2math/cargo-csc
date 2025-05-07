// mod v4;

mod spec;

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
