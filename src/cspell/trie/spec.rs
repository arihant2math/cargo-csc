use std::cell::UnsafeCell;
use std::io::Read;
use std::ops::{Index, IndexMut};
use anyhow::bail;
use flate2::bufread::GzDecoder;

#[derive(Debug)]
struct Version(pub String);

impl Version {
    // TODO: Should be result due to unwrap
    pub fn to_u8(&self) -> u8 {
        self.0
            .split('v')
            .last()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap()
    }
}

#[derive(Debug)]
struct Header {
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
    children: std::collections::HashMap<char, *mut TrieNode>,
}

struct Stack {
    inner: UnsafeCell<Vec<TrieNode>>
}

impl Stack {
    fn new() -> Self {
        Stack {
            inner: UnsafeCell::new(Vec::new()),
        }
    }

    fn get(&self) -> &mut Vec<TrieNode> {
        unsafe { &mut *self.inner.get() }
    }

    unsafe fn get_index_hack_mut(&self, index: usize) -> &mut TrieNode {
        &mut self.get()[index]
    }

    fn push(&self, node: TrieNode) {
        self.get().push(node);
    }

    fn pop(&mut self) -> TrieNode {
        self.get().pop().unwrap()
    }

    fn len(&self) -> usize {
        self.get().len()
    }
}

impl Index<usize> for Stack {
    type Output = TrieNode;

    fn index(&self, index: usize) -> &Self::Output {
        &self.get()[index]
    }
}

impl IndexMut<usize> for Stack {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.get()[index]
    }
}

fn parse_body(input: &[String], header: &Header) -> crate::Trie {
    let mut stack = Stack::new();

    let root = TrieNode {
        eow: false,
        children: std::collections::HashMap::new(),
    };
    stack.push(root);
    enum State {
        Escape,
        Remove,
        InId {
            chars: Vec<char>,
        },
        InWord
    }
    let mut state = State::InWord;
    let mut pos: Vec<*mut TrieNode> = vec![&mut stack[0]];
    for line in input {
        for char in line.chars() {
            if char == '\n' {
                continue;
            }
            if matches!(state, State::Escape) {
                state = State::InWord;
                let last: &mut TrieNode = unsafe { &mut **pos.last().unwrap() };
                if last.children.contains_key(&char) {
                    unsafe {
                        pos.push(last.children[&char]);
                    }
                } else {
                    let new_node = TrieNode {
                        eow: false,
                        children: std::collections::HashMap::new(),
                    };
                    drop(last);
                    stack.push(new_node);
                    let last: &mut TrieNode = unsafe { &mut **pos.last().unwrap() };
                    unsafe {
                        last.children.insert(char, stack.get_index_hack_mut(stack.len() - 1));
                        pos.push(stack.get_index_hack_mut(stack.len() - 1));
                    }
                }
                continue;
            }

            let to_remove = if matches!(state, State::Remove) && char.is_numeric() {
                char.to_digit(10).unwrap()
            } else if matches!(state, State::Remove) {
                1
            } else {
                0
            };
            if to_remove > 0 {
                state = State::InWord;
                for _ in 0..to_remove {
                    pos.pop();
                }
                continue;
            }

            if let State::InId { chars } = &mut state {
                if char.is_numeric() {
                    chars.push(char);
                    continue;
                } else {
                    assert_eq!(char, ';');
                    // convert chars to number with header specified base
                    let number = chars
                        .iter()
                        .collect::<String>();
                    // convert to number in header.base
                    let number = u32::from_str_radix(&number, header.base as u32)
                        .expect("Failed to convert number");
                    // TODO: This might not be kosher
                    pos = vec![unsafe { stack.get_index_hack_mut(number as usize) }];
                    state = State::InWord;
                    continue;
                }
            }

            if char == '\\' {
                state = State::Escape;
                continue;
            } else if char == '$' {
                let ptr: &mut TrieNode = unsafe { &mut **pos.last().unwrap() };
                ptr.eow = true;
            } else if char == '<' {
                state = State::Remove;
            } else {
                let last: &mut TrieNode = unsafe { &mut **pos.last().unwrap() };
                if last.children.contains_key(&char) {
                    unsafe {
                        pos.push(last.children[&char]);
                    }
                } else {
                    let new_node = TrieNode {
                        eow: false,
                        children: std::collections::HashMap::new(),
                    };
                    drop(last);
                    stack.push(new_node);
                    let last: &mut TrieNode = unsafe { &mut **pos.last().unwrap() };
                    unsafe {
                        last.children.insert(char, stack.get_index_hack_mut(stack.len() - 1));
                        pos.push(stack.get_index_hack_mut(stack.len() - 1));
                    }
                }
            }
        }
    }

    fn node_from_bool(eow: bool) -> crate::trie::TrieNode {
        if eow {
            crate::trie::TrieNode::some_default()
        } else {
            crate::trie::TrieNode::none()
        }
    }

    fn convert(node: *const TrieNode, stack: &Stack) -> crate::trie::TrieNode {
        let node = unsafe { &*node };
        let mut conv_node = node_from_bool(node.eow);
        for (c, ptr) in &node.children {
            let out = convert(*ptr, stack);
            conv_node.children.insert(*c, out);
        }
        conv_node
    }

    let mut conv_root = node_from_bool(stack[0].eow);
    for (c, ptr) in &stack[0].children {
        let out = convert(*ptr, &stack);
        conv_root.children.insert(*c, out);
    }
    crate::Trie {
        root: conv_root,
        options: Default::default(),
    }
}

pub fn parse_trie(input: &[String]) -> anyhow::Result<(Header, crate::Trie)> {
    let (counter, header) = parse_header(input)?;
    let body = &input[counter..];
    dbg!();
    let trie = parse_body(body, &header);
    Ok((header, trie))
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
        .map(|s| s.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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

    #[test]
    fn test_parse_body() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec![
            "a$".to_string(),
            "b$".to_string(),
            "c$".to_string(),
        ];
        let trie = parse_body(&input, &header);
        assert!(trie.contains("a"));
        assert!(!trie.contains("b"));
        assert!(!trie.contains("c"));
        assert!(!trie.contains("d"));
        assert!(trie.contains("ab"));
        assert!(trie.contains("abc"));
    }

    #[test]
    fn test_parse_en_US() {
        let path = r"C:\Users\ariha\.code-spellcheck\tmp\cspell-dicts\dictionaries\en_US\en_US.trie";
        let lines = file_to_lines(path).unwrap();
        let (header, trie) = parse_trie(&lines).unwrap();
        assert_eq!(header.version.to_u8(), 3);
    }
}
