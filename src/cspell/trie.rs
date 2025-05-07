// mod v4;

mod spec;

use crate::Trie;

trait CspellTrieVersion {
    fn read(lines: &[String]) -> anyhow::Result<crate::trie::TrieNode>;

    fn write(trie: &crate::Trie) -> anyhow::Result<Vec<String>>;
}

struct V3;

impl CspellTrieVersion for V3 {
    fn read(lines: &[String]) -> anyhow::Result<crate::trie::TrieNode> {
        // let res = v4::import_trie(lines);
        // Ok(res.into())
        todo!()
    }

    fn write(trie: &Trie) -> anyhow::Result<Vec<String>> {
        todo!()
    }
}

struct V4;

impl CspellTrieVersion for V4 {
    fn read(lines: &[String]) -> anyhow::Result<crate::trie::TrieNode> {
        // let res = v4::import_trie(lines);
        // Ok(res.into())
        todo!()
    }

    fn write(trie: &Trie) -> anyhow::Result<Vec<String>> {
        todo!()
    }
}
