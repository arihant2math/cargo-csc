use crate::Trie;
use flate2::bufread::GzDecoder;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Read,
    rc::Rc,
};

#[derive(Debug)]
struct Version(pub String);

impl Version {
    // TODO: Should be result due to unwrap
    pub fn to_u8(&self) -> u8 {
        self.0
            .split('v')
            .next_back()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap()
    }
}

#[derive(Debug)]
pub struct Header {
    version: Version,
    base: u8,
}

fn parse_header(input: &[String]) -> anyhow::Result<(usize, Header)> {
    let mut counter = 0;
    let mut version = None;
    let mut base = None;
    loop {
        let line = input.get(counter);
        counter += 1;
        let line = match line {
            Some(line) => line,
            None => break,
        };
        if line.ends_with("__DATA__") {
            break;
        }
        if line.starts_with("TrieXv") {
            let rest = line.strip_prefix("TrieXv").unwrap();
            version = Some(rest.to_string());
        }
        if line.starts_with("base=") {
            let rest = line.strip_prefix("base=").unwrap();
            base = Some(rest.parse::<u8>()?);
        }
    }
    Ok((
        counter,
        Header {
            version: Version(version.unwrap()),
            base: base.unwrap(),
        },
    ))
}

struct TrieNode {
    eow: bool,
    parent: Option<Rc<RefCell<TrieNode>>>,
    children: HashMap<char, Rc<RefCell<TrieNode>>>,
}

/// Internal parse states.
#[derive(Debug)]
enum ParseState {
    InWord,
    Escape,
    Remove,
    AbsoluteReference { chars: Vec<char> },
    // TODO: impl
    // RelRef { chars: Vec<char> },
}

/// Helper struct that builds a trie.
struct TrieBuilder {
    /// Flat storage of nodes (for reference indexing).
    nodes: Vec<Rc<RefCell<TrieNode>>>,
    /// Current path in the tree.
    pos: Vec<Rc<RefCell<TrieNode>>>,
}

impl TrieBuilder {
    fn new() -> Self {
        let root = Rc::new(RefCell::new(TrieNode::new_root()));
        Self {
            nodes: vec![root.clone()],
            pos: vec![root],
        }
    }

    fn dbg_state(&self) {
        fn bstr(b: bool) -> String {
            if b { "*".to_string() } else { " ".to_string() }
        }
        for (i, node) in self.nodes.iter().enumerate() {
            let pos_pos = self
                .pos
                .iter()
                .position(|p| Rc::ptr_eq(&p, &node))
                .map(|p| p as i64)
                .unwrap_or(-1);
            let mut c = self.get_char(node);
            let node_borrow = node.borrow();
            let mut child_ids: Vec<_> = node_borrow
                .children
                .values()
                .map(|v| {
                    // Find position in nodes
                    self.nodes
                        .iter()
                        .position(|p| Rc::ptr_eq(&p, &v))
                        .unwrap_or(usize::MAX)
                })
                .collect();
            child_ids.sort();
            let children = child_ids
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "{pos_pos:>2} {c} ID {:>3}: {} children={}",
                i,
                bstr(node_borrow.eow),
                children
            );
        }
    }

    fn get_char(&self, node: &Rc<RefCell<TrieNode>>) -> char {
        // Get parent
        let node_borrow = node.borrow();
        let parent = node.borrow().parent.clone();
        if let Some(parent) = parent {
            let parent_borrow = parent.borrow();
            for (c, child) in &parent_borrow.children {
                if Rc::ptr_eq(&child, node) {
                    return *c;
                }
            }
        }
        ' '
    }

    /// Absolute jump to a node in the trie.
    fn jump_to(&mut self, idx: usize) {
        // 1) Peek, don’t pop:
        let last = self.pos.last().unwrap().clone();

        // 2) Get the char and parent:
        let ch = self.get_char(&last);
        let parent = last.borrow().parent.clone().unwrap();

        // 3) Redirect that slot, _in-place_:
        parent.borrow_mut().children.insert(ch, Rc::clone(&self.nodes[idx]));

        // 4) Replace the top of the stack with the referenced node:
        *self.pos.last_mut().unwrap() = Rc::clone(&self.nodes[idx]);
    }

    /// Process a single character and update state.
    fn process_char(&mut self, c: char, header_base: u32, state: &mut ParseState) {
        dbg!(c, &state);
        self.dbg_state();
        match state {
            ParseState::Escape => {
                self.add_char(c);
                *state = ParseState::InWord;
            }
            ParseState::Remove => {
                let count = if c.is_numeric() {
                    let out = c.to_digit(10).unwrap();
                    // As per the spec, out can't be 1
                    assert_ne!(out, 1);
                    out
                } else {
                    1
                };
                for _ in 0..count {
                    if self.pos.pop().is_none() {
                        self.dbg_state();
                        panic!("No more nodes to pop");
                    }
                }
                if count == 1 {
                    match c {
                        '\\' => *state = ParseState::Escape,
                        '$' => {
                            if let Some(cur) = self.pos.last() {
                                cur.borrow_mut().eow = true;
                            }
                            *state = ParseState::Remove;
                        }
                        '<' => *state = ParseState::Remove,
                        '#' => {
                            *state = ParseState::AbsoluteReference { chars: vec![c] };
                        }
                        _ => {
                            self.add_char(c);
                            *state = ParseState::InWord;
                        },
                    }
                }
            }
            ParseState::AbsoluteReference { chars } => {
                if c == ';' {
                    let number_str: String = chars.iter().collect();
                    let idx = u32::from_str_radix(&number_str[1..], header_base)
                        .expect("Failed to convert number") as usize;
                    if idx < self.nodes.len() {
                        self.jump_to(idx);
                    } else {
                        self.dbg_state();
                        panic!("Index out of bounds: {}", idx);
                    }
                    *state = ParseState::InWord;
                } else {
                    chars.push(c);
                }
            }
            ParseState::InWord => match c {
                '\\' => *state = ParseState::Escape,
                '$' => {
                    if let Some(cur) = self.pos.last() {
                        cur.borrow_mut().eow = true;
                    }
                    *state = ParseState::Remove;
                }
                '<' => *state = ParseState::Remove,
                '#' => {
                    *state = ParseState::AbsoluteReference { chars: vec![c] };
                }
                _ => self.add_char(c),
            },
        }
    }

    /// Add a character as a child node to the last node in the current path.
    fn add_char(&mut self, c: char) {
        if let Some(parent) = self.pos.last().cloned() {
            let mut parent_borrow = parent.borrow_mut();
            if let Some(child) = parent_borrow.children.get(&c) {
                self.pos.push(child.clone());
            } else {
                // TODO: causes leak
                let new_node = Rc::new(RefCell::new(TrieNode::new(parent.clone(), false)));
                parent_borrow.children.insert(c, new_node.clone());
                self.nodes.push(new_node.clone());
                self.pos.push(new_node);
            }
        } else {
            self.dbg_state();
            unreachable!();
        }
    }
}

impl TrieNode {
    /// Create a new TrieNode.
    fn new(parent: Rc<RefCell<Self>>, eow: bool) -> Self {
        TrieNode {
            eow,
            children: HashMap::new(),
            parent: Some(parent),
        }
    }

    fn new_root() -> Self {
        TrieNode {
            eow: false,
            children: HashMap::new(),
            parent: None,
        }
    }

    fn deep_clone(&self) -> Self {
        let mut new_node = TrieNode::new_root();
        new_node.eow = self.eow;
        for (c, child) in &self.children {
            let child_clone = child.borrow().deep_clone();
            new_node.children.insert(*c, Rc::new(RefCell::new(child_clone)));
        }
        new_node
    }
}

/// Recursively convert the builder trie into the output Trie structure.
fn convert_trie(builder_root: Rc<RefCell<TrieNode>>) -> Trie {
    fn rec_convert(node: &Rc<RefCell<TrieNode>>) -> crate::trie::TrieNode {
        let node_ref = node.borrow();
        let mut out = if node_ref.eow {
            crate::trie::TrieNode::some_default()
        } else {
            crate::trie::TrieNode::none()
        };
        for (&c, child) in &node_ref.children {
            out.children.insert(c, rec_convert(child));
        }
        out
    }
    let root_converted = rec_convert(&builder_root);
    Trie {
        root: root_converted,
        options: Default::default(),
    }
}

/// Refactored parse_body function.
pub fn parse_body(input: &[String], header: &Header) -> crate::Trie {
    let mut builder = TrieBuilder::new();
    let mut state = ParseState::InWord;
    let header_base = header.base as u32;

    for line in input {
        for ch in line.chars() {
            if ch == '\n' {
                continue;
            }
            builder.process_char(ch, header_base, &mut state);
        }
    }
    let root = builder.nodes.first().unwrap().clone();
    convert_trie(root)
}

pub fn parse_trie(input: &[String]) -> anyhow::Result<(Header, crate::Trie)> {
    let (counter, header) = parse_header(input)?;
    let body = &input[counter..];
    let trie = parse_body(body, &header);
    Ok((header, trie))
}

pub fn file_to_lines<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Vec<String>> {
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
    Ok(text.lines().map(|s| s.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    fn test_parse_header() {
        let input = vec![
            "TrieXv4".to_string(),
            "base=10".to_string(),
            "__DATA__".to_string(),
        ];
        let (counter, header) = parse_header(&input).unwrap();
        assert_eq!(counter, 3);
        assert_eq!(header.version.to_u8(), 4);
        assert_eq!(header.base, 10);
    }

    // #[test]
    fn test_parse_body_basic() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec!["a$".to_string(), "b$".to_string(), "c$".to_string()];
        let trie = parse_body(&input, &header);
        assert!(trie.contains("a"));
        assert!(trie.contains("b"));
        assert!(trie.contains("c"));
        assert!(!trie.contains("d"));
        assert!(!trie.contains("ab"));
        assert!(!trie.contains("abc"));
    }

    #[test]
    fn test_small() {
        let path = r"D:\Documents\Programming\cargo-csc\test.trie";
        let lines = file_to_lines(path).unwrap();
        let (header, trie) = parse_trie(&lines).unwrap();
        let v = trie.to_vec();
        for word in &v {
            println!("{}", word);
        }
        assert_eq!(header.version.to_u8(), 3);
        assert!(v.contains(&"'cause".to_string()));
    }

    // #[test]
    fn test_parse_en_us() {
        let path =
            r"C:\Users\ariha\.code-spellcheck\tmp\cspell-dicts\dictionaries\en_US\en_US.trie";
        let lines = file_to_lines(path).unwrap();
        let (header, trie) = parse_trie(&lines).unwrap();
        dbg!(&trie.to_vec());
        assert_eq!(header.version.to_u8(), 3);
        assert!(trie.contains("'cause'"))
    }
}
