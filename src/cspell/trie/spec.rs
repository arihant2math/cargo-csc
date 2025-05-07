use std::io::Read;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use flate2::bufread::GzDecoder;

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
    children: std::collections::HashMap<char, Arc<Mutex<TrieNode>>>,
}

fn parse_body(input: &[String], header: &Header) -> crate::Trie {
    let stack = Mutex::new(Vec::new());

    let root = TrieNode {
        eow: false,
        children: std::collections::HashMap::new(),
    };
    let mut unlock = stack.lock().unwrap();
    unlock.push(Arc::new(Mutex::new(root)));
    enum State {
        Escape,
        Remove,
        InId {
            chars: Vec<char>,
        },
        InWord
    }
    let mut state = State::InWord;
    let mut pos = Mutex::new(vec![unlock[0].clone()]);
    drop(unlock);
    for line in input {
        for char in line.chars() {
            let mut unlock = stack.lock().unwrap();
            if char == '\n' {
                continue;
            }
            if matches!(state, State::Escape) {
                state = State::InWord;
                let pos_lock = pos.lock().unwrap();
                let last = pos_lock.last().unwrap().clone();
                drop(pos_lock);
                let last_lock = last.lock().unwrap();
                if last_lock.children.contains_key(&char) {
                    let mut pos_lock = pos.lock().unwrap();
                    pos_lock.push(last_lock.children[&char].clone());
                } else {
                    let new_node = TrieNode {
                        eow: false,
                        children: std::collections::HashMap::new(),
                    };
                    unlock.push(Arc::new(Mutex::new(new_node)));
                    let pos_lock = pos.lock().unwrap();
                    let last = pos_lock.last().unwrap().clone();
                    let mut last_lock = last.lock().unwrap();
                    drop(pos_lock);
                    last_lock.children.insert(char, unlock.last().unwrap().clone());
                    let mut pos_lock = pos.lock().unwrap();
                    pos_lock.push(unlock.last().unwrap().clone());
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
                let mut pos_lock = pos.lock().unwrap();
                for _ in 0..to_remove {
                    pos_lock.pop();
                }
                drop(pos_lock);
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
                    pos = Mutex::new(vec![unlock[number as usize].clone()]);
                    state = State::InWord;
                    continue;
                }
            }

            if char == '\\' {
                state = State::Escape;
                continue;
            } else if char == '$' {
                let pos_lock = pos.lock().unwrap();
                let last = pos_lock.last().unwrap();
                let mut last_lock = last.lock().unwrap();
                last_lock.eow = true;
            } else if char == '<' {
                state = State::Remove;
            } else {
                let pos_lock = pos.lock().unwrap();
                let last = pos_lock.last().unwrap().clone();
                drop(pos_lock);
                let last_lock = last.lock().unwrap();
                if last_lock.children.contains_key(&char) {
                    let mut pos_lock = pos.lock().unwrap();
                    pos_lock.push(last_lock.children[&char].clone());
                } else {
                    let new_node = TrieNode {
                        eow: false,
                        children: std::collections::HashMap::new(),
                    };
                    unlock.push(Arc::new(Mutex::new(new_node)));
                    let pos_lock = pos.lock().unwrap();
                    let last = pos_lock.last().unwrap().clone();
                    drop(pos_lock);
                    let mut last_lock = last.lock().unwrap();
                    last_lock.children.insert(char, unlock.last().unwrap().clone());
                    let mut pos_lock = pos.lock().unwrap();
                    pos_lock.push(unlock.last().unwrap().clone());
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

    fn convert(node: Arc<Mutex<TrieNode>>, stack: &Vec<Arc<Mutex<TrieNode>>>) -> crate::trie::TrieNode {
        let node = node.lock().unwrap();
        let mut conv_node = node_from_bool(node.eow);
        for (c, ptr) in &node.children {
            let out = convert(ptr.clone(), stack);
            conv_node.children.insert(*c, out);
        }
        conv_node
    }

    let l_stack = stack.lock().unwrap();
    let root = l_stack.first().unwrap().lock().unwrap();
    let mut conv_root = node_from_bool(root.eow);
    for (c, ptr) in &root.children {
        let out = convert(ptr.clone(), l_stack.deref());
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
    fn test_parse_en_us() {
        let path = r"C:\Users\ariha\.code-spellcheck\tmp\cspell-dicts\dictionaries\en_US\en_US.trie";
        let lines = file_to_lines(path).unwrap();
        let (header, trie) = parse_trie(&lines).unwrap();
        assert_eq!(header.version.to_u8(), 3);
    }
}
