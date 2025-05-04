use crate::Trie;

#[derive(Clone, Debug, Default)]
pub struct MultiTrie {
    pub inner: Vec<Trie>,
}

impl MultiTrie {
    pub fn new() -> Self {
        MultiTrie { inner: Vec::new() }
    }

    pub fn contains(&self, word: &str) -> bool {
        for trie in &self.inner {
            if trie.contains(word) {
                return true;
            }
        }
        false
    }

    fn check_parts(&self, parts: &[&str]) -> bool {
        fn split_by_capitalization(word: &str) -> Vec<String> {
            let mut parts = Vec::new();
            let mut current_part = String::new();
            for c in word.chars() {
                if c.is_uppercase() && !current_part.is_empty() {
                    parts.push(current_part);
                    current_part = String::new();
                }
                current_part.push(c);
            }
            if !current_part.is_empty() {
                parts.push(current_part);
            }
            parts
        }

        for part in parts {
            if !self.contains(&part.to_ascii_lowercase()) {
                // check if part is fully numeric
                if part.chars().all(|c| c.is_numeric()) {
                    continue;
                } else {
                    let mut found = true;
                    for sub_part in split_by_capitalization(part) {
                        if !self.contains(&sub_part.to_ascii_lowercase()) {
                            found = false;
                            break;
                        }
                    }
                    if found {
                        continue;
                    } else {
                        println!("Parts: {:?}", &parts);
                        println!("Word not found: {}", part);
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn handle_identifier(&self, word: &str) -> bool {
        let splitters = [
            ' ', '_', '-', '\n', '\t', '(', ')', '{', '}', '[', ']', ',', '.', ';', ':', '?', '!',
            '"', '\'', '&', '/', '\\', '|', '<', '>', '=', '+', '-', '*', '%', '^', '~', '`', '@',
            '#', '$', '!', '?', ':', ';', '(', ')', '{', '}', '[', ']', ',', '.', '/', '\\',
        ];
        let parts = word
            .split(|c| splitters.contains(&c))
            .filter(|part| part.len() > 1)
            .collect::<Vec<_>>();
        self.check_parts(&parts)
    }
}
