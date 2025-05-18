use std::{cell::OnceCell, sync::Arc};

use crate::Trie;

#[derive(Debug, Default)]
pub struct MultiTrie {
    pub inner: Vec<Arc<Trie>>,
    pub all_words: OnceCell<Vec<String>>,
}

impl MultiTrie {
    pub fn new() -> Self {
        MultiTrie {
            inner: Vec::new(),
            all_words: OnceCell::new(),
        }
    }

    pub fn contains(&self, word: &str) -> bool {
        for trie in &self.inner {
            if trie.contains(word) {
                return true;
            }
        }
        false
    }

    fn check_parts(&self, parts: &[&str]) -> Option<String> {
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

        for &part in parts {
            if !self.contains(&part.to_ascii_lowercase()) {
                // check if part is fully numeric
                if !part.chars().all(char::is_numeric) {
                    for sub_part in split_by_capitalization(part) {
                        if !self.contains(&sub_part.to_ascii_lowercase()) {
                            return Some(part.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    pub fn handle_identifier(&self, word: &str) -> Option<String> {
        let splitters = [
            ' ', '_', '-', '(', ')', '{', '}', '[', ']', ',', '.', ';', ':', '?', '!', '"', '\'',
            '&', '/', '|', '<', '>', '=', '+', '-', '*', '%', '^', '~', '`', '@', '#', '$', '!',
            '?', ':', ';', '(', ')', '{', '}', '[', ']', ',', '.', '/', '1', '2', '3', '4', '5',
            '6', '7', '8', '9', '0', '\\',
        ];
        // TODO: handle \ properly
        let parts = word
            .split(|c| splitters.contains(&c))
            .filter(|part| part.len() > 3)
            .collect::<Vec<_>>();
        self.check_parts(&parts)
    }

    pub fn suggestion(&self, word: &str) -> Option<String> {
        const THRESHOLD: f64 = 0.7;

        let (score, best_suggestion) = self
            .inner
            .iter()
            .filter_map(|t| t.check(word).unwrap())
            .filter_map(|suggestion| {
                let score = strsim::normalized_damerau_levenshtein(word, &suggestion);
                Some((score, suggestion))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))?;
        if score > THRESHOLD {
            Some(best_suggestion)
        } else {
            None
        }
    }
}
