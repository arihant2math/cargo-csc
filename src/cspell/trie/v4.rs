//! Trie file format v4 in Rust
//! Ported from TypeScript implementation

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Read;
use flate2::bufread::GzDecoder;

pub const BACK: char = '<'; // placeholder
pub const EOL: char = '\n';
pub const EOR: char = '\r';
pub const EOW: char = '$';
pub const ESCAPE: char = '\\';
pub const LF: char = '\n';
pub const REF: char = '@';
pub const REF_REL: char = '#';

const REF_INDEX_BEGIN: char = '[';
const REF_INDEX_END: char = ']';
const INLINE_DATA_COMMENT_LINE: char = '/';
const WORDS_PER_LINE: usize = 20;

/// Trie node representation
#[derive(Clone, Debug, Default)]
pub struct CSpellTrieNode {
    pub f: bool,
    pub c: Option<HashMap<char, Box<CSpellTrieNode>>>,
}

pub type CSpellTrieRoot = CSpellTrieNode;

impl Into<crate::trie::TrieNode> for CSpellTrieRoot {
    fn into(self) -> crate::trie::TrieNode {
        let mut trie_node = crate::trie::TrieNode::default();
        if self.f {
            trie_node.data = Some(crate::trie::TrieData { disallow: false });
        }
        if let Some(children) = self.c {
            trie_node.children = children
                .into_iter()
                .map(|(k, v)| (k, (*v).into()))
                .collect();
        }
        trie_node
    }
}

/// Export options
pub struct ExportOptions {
    pub base: usize,
    pub comment: String,
    pub optimize_simple_references: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            base: 10,
            comment: String::new(),
            optimize_simple_references: false,
        }
    }
}

/// Serialize a TrieRoot to V4 format
pub fn serialize_trie(root: &CSpellTrieRoot, opts: Option<ExportOptions>) -> Vec<String> {
    // ... existing serialization logic ...
    unimplemented!()
}

/// Deserialize lines of a V3/V4 trie into a TrieRoot
pub fn import_trie(lines: &[String]) -> CSpellTrieRoot {
    // Read header and determine radix
    let mut radix = 10usize;
    // clone each line into the iterator (so that the `.chars()` iterator owns its String)
    let mut iter = lines
        .iter()
        .cloned() // own each String
        .flat_map(|line| {
            let line_chars: Vec<char> = line.chars().clone().collect();
            line_chars.into_iter().chain(std::iter::once('\n'))
        });
    // Skip comments and header until DATA marker
    let mut header = String::new();
    while let Some(ch) = iter.next() {
        // accumulate header lines
        header.push(ch);
        if header.ends_with("__DATA__\n") {
            break;
        }
    }
    // parse base from header
    for line in header.lines() {
        if let Some(rest) = line.strip_prefix("base=") {
            radix = rest.parse().unwrap_or(10);
        }
    }

    // Read reference index JSON array
    let mut ref_json = String::new();
    // consume until '['
    while let Some(ch) = iter.next() {
        if ch == REF_INDEX_BEGIN {
            ref_json.push(ch);
            break;
        }
    }
    // collect until ']' inclusive
    while let Some(ch) = iter.next() {
        ref_json.push(ch);
        if ch == REF_INDEX_END {
            break;
        }
    }
    // parse indices
    let ref_index: Vec<usize> = ref_json
        .trim_matches(&['[', ']'][..])
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // Create root and parser state
    let mut root = CSpellTrieNode::default();
    let eow_node = Box::new(CSpellTrieNode { f: true, c: None });
    let mut stack: Vec<(*mut CSpellTrieNode, char)> = Vec::new();
    let root_ptr: *mut CSpellTrieNode = &mut root;
    unsafe {
        stack.push((root_ptr, '\0'));
    }
    let mut nodes: Vec<*mut CSpellTrieNode> = vec![root_ptr];

    enum State {
        Main,
        InNumber { is_rel: bool, buf: String },
        InEscape,
        InComment { end_char: char, escaped: bool },
    }
    let mut state = State::Main;

    for ch in iter {
        state = match std::mem::replace(&mut state, State::Main) {
            State::Main => match ch {
                ESCAPE => State::InEscape,
                EOW => {
                    // mark current node as word
                    let (node_ptr, _) = stack.last().unwrap();
                    unsafe {
                        (*(*node_ptr)).f = true;
                    }
                    // if no children, replace with eow_node
                    State::Main
                }
                BACK => {
                    // pop one
                    stack.pop();
                    State::Main
                }
                REF | REF_REL => State::InNumber {
                    is_rel: ch == REF_REL,
                    buf: String::new(),
                },
                INLINE_DATA_COMMENT_LINE => State::InComment {
                    end_char: EOL,
                    escaped: false,
                },
                EOL | LF => State::Main,
                _ => {
                    // regular character: add node
                    let &(parent_ptr, _) = stack.last().unwrap();
                    let parent = unsafe { &mut *parent_ptr };
                    let mut child = CSpellTrieNode::default();
                    let ch_copy = ch;
                    parent
                        .c
                        .get_or_insert_with(HashMap::new)
                        .insert(ch_copy, Box::new(child));
                    let new_ptr: *mut CSpellTrieNode = {
                        let map = parent.c.as_mut().unwrap();
                        &mut **map.get_mut(&ch_copy).unwrap() as *mut _
                    };
                    stack.push((new_ptr, ch_copy));
                    nodes.push(new_ptr);
                    State::Main
                }
            },
            State::InEscape => {
                // consume next char literally
                let real = ch;
                // treat as in Main with real
                state = State::Main;
                State::Main
            }
            State::InNumber { is_rel, mut buf } => {
                if ch.is_digit(radix as u32) {
                    buf.push(ch);
                    State::InNumber { is_rel, buf }
                } else {
                    // complete number
                    if let Ok(idx) = usize::from_str_radix(&buf, radix as u32) {
                        let target = if is_rel { ref_index[idx] } else { idx };
                        let node_ptr = nodes[target];
                        // attach reference: treat as child of parent
                        let &(parent_ptr, ch_prev) = stack.last().unwrap();
                        unsafe {
                            let parent = &mut *parent_ptr;
                            parent
                                .c
                                .get_or_insert_with(HashMap::new)
                                .insert(ch_prev, Box::new((*node_ptr).clone()));
                        }
                    }
                    // re-process ch in Main
                    if ch == EOR {
                        State::Main
                    } else {
                        // replay ch
                        if ch == ESCAPE {
                            State::InEscape
                        } else {
                            State::Main
                        }
                    }
                }
            }
            State::InComment { end_char, escaped } => {
                if escaped {
                    state = State::InComment {
                        end_char,
                        escaped: false,
                    };
                    State::InComment {
                        end_char,
                        escaped: false,
                    }
                } else if ch == ESCAPE {
                    State::InComment {
                        end_char,
                        escaped: true,
                    }
                } else if ch == end_char {
                    State::Main
                } else {
                    State::InComment {
                        end_char,
                        escaped: false,
                    }
                }
            }
        };
    }

    root
}

pub fn file_to_lines<P: AsRef<std::path::Path>>(
    path: P,
) -> std::io::Result<Vec<String>> {
    // Read the entire file into a byte buffer
    let buf = std::fs::read(&path)?;
    let filename = path.as_ref().to_string_lossy();

    // Decode if gzipped, otherwise assume UTF-8 text
    let text = if filename.ends_with(".gz") {
        let mut decoder = GzDecoder::new(&buf[..]);
        let mut s = String::new();
        decoder.read_to_string(&mut s)?;
        s
    } else {
        String::from_utf8(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
    };

    // Split on newlines (`lines()` handles `\n` and `\r\n`)
    Ok(text.lines()
        .map(|s| s.to_string() + "\n")
        .collect())
}
