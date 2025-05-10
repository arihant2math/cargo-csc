use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use regex::Regex;

pub const FLAG_WORD: u32 = 1;
pub const EOW: char = '$';
pub const BACK: char = '<';
pub const EOL: char = '\n';
pub const LF: char = '\r';
pub const REF: char = '#';
pub const REF_REL: char = '@';
pub const EOR: char = ';';
pub const ESCAPE: char = '\\';
const REF_INDEX_BEGIN: char = '[';
const REF_INDEX_END: char = ']';
const INLINE_DATA_COMMENT_LINE: char = '/';
const DATA: &str = "__DATA__";

fn string_to_char_set(s: &str) -> HashMap<char, ()> {
    let mut set = HashMap::new();
    for c in s.chars() {
        set.insert(c, ());
    }
    set
}

fn create_lookup_map<T: Clone>(pairs: &[(char, T)]) -> HashMap<char, T> {
    let mut m = HashMap::new();
    for (k, v) in pairs.iter() {
        m.insert(*k, v.clone());
    }
    m
}

fn special_char_map() -> HashMap<String, char> {
    let arr = [('\n', "\\n"), ('\r', "\\r"), ('\\', "\\\\")];
    let mut m = HashMap::new();
    for (c, s) in arr.iter() {
        m.insert(s.to_string(), *c);
    }
    m
}

pub type ChildMap = HashMap<char, TrieNodePtr>;
pub type TrieNodePtr = Rc<RefCell<TrieNode>>;

pub struct TrieInfo {
    pub compound_character: String,
    pub strip_case_and_accents_prefix: String,
    pub forbidden_word_prefix: String,
    pub is_case_aware: bool,
}

pub struct TrieNode {
    pub f: Option<u32>,
    pub c: Option<ChildMap>,
}

pub struct TrieRoot {
    pub info: TrieInfo,
    pub c: ChildMap,
}

struct StackItem {
    node: TrieNodePtr,
    ch: char,
}

struct ReduceResults {
    stack: Vec<StackItem>,
    nodes: Vec<TrieNodePtr>,
    root: TrieRoot,
    parser: Option<fn(&mut ReduceResults, char)>,
}

pub fn import_trie(lines: impl IntoIterator<Item=String>) -> TrieRoot {
    let mut radix = 10;
    let comment_re = Regex::new(r"^\s*#").unwrap();
    let mut iter = Vec::new();
    for line in lines {
        for seg in line.split_inclusive('\n') {
            iter.push(seg.to_string());
        }
    }
    let mut header_rows = Vec::new();
    let mut idx = 0;
    while idx < iter.len() {
        let line = iter[idx].trim();
        idx += 1;
        if line.is_empty() || comment_re.is_match(line) { continue; }
        if line == DATA { break; }
        header_rows.push(line.to_string());
    }
    parse_header(&header_rows, &mut radix);
    let rest = iter[idx..].iter().cloned().collect::<Vec<_>>();
    parse_stream(radix, rest)
}

fn parse_header(rows: &[String], radix: &mut usize) {
    let header = rows.join("\n");
    let re = Regex::new(r"^TrieXv[34]\nbase=(\d+)$").unwrap();
    if let Some(cap) = re.captures(&header) {
        *radix = cap[1].parse().unwrap();
    } else {
        panic!("Unknown file format");
    }
}

fn parse_stream(radix: usize, lines: Vec<String>) -> TrieRoot {
    let special_map = special_char_map();
    let numbers = string_to_char_set("0123456789");
    let spaces = string_to_char_set(" \r\n\t");

    let eow_node = Rc::new(RefCell::new(TrieNode { f: Some(FLAG_WORD), c: None }));
    let mut ref_index: Vec<usize> = Vec::new();

    // initialize root
    let root_info = TrieInfo {
        compound_character: String::new(),
        strip_case_and_accents_prefix: String::new(),
        forbidden_word_prefix: String::new(),
        is_case_aware: false,
    };
    let root_ptr = Rc::new(RefCell::new(TrieNode { f: None, c: Some(HashMap::new()) }));
    let mut root = TrieRoot { info: root_info, c: HashMap::new() };
    root.c = root_ptr.borrow().c.clone().unwrap();

    // State
    let mut acc = ReduceResults {
        stack: vec![StackItem { node: root_ptr.clone(), ch: '\0' }],
        nodes: vec![root_ptr.clone()],
        root: root,
        parser: Some(parse_ref_index),
    };

    fn parser_main(acc: &mut ReduceResults, ch: char,
                   map: &HashMap<String, char>, nums: &HashMap<char, ()>, spaces: &HashMap<char, ()>) {
        let p = acc.parser.take();
        let next = if let Some(func) = p {
            func(acc, ch)
        } else {
            default_parse(acc, ch)
        };
        acc.parser = next;
    }

    // Character handlers:
    fn default_parse(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
        match ch {
            EOW => { parse_eow(acc, ch); Some(parse_back) },
            BACK | '2'..='9' => Some(parse_back),
            REF | REF_REL => { parse_reference(acc, ch); None },
            ESCAPE => { Some(parse_escape) },
            EOL | LF => None,
            INLINE_DATA_COMMENT_LINE => { Some(parse_comment) },
            c => { parse_char(acc, c); None }
        }
    }

    fn parse_char(acc: &mut ReduceResults, ch: char) {
        let mut node = acc.stack.last().unwrap().node.borrow_mut();
        let mut cmap = node.c.take().unwrap_or_default();
        let new_node = Rc::new(RefCell::new(TrieNode { f: None, c: None }));
        cmap.insert(ch, new_node.clone());
        node.c = Some(cmap);
        drop(node);
        acc.stack.push(StackItem { node: new_node.clone(), ch });
        acc.nodes.push(new_node);
    }

    fn parse_eow(acc: &mut ReduceResults, _: char) {
        let top = acc.stack.pop().unwrap();
        top.node.borrow_mut().f = Some(FLAG_WORD);
        if top.node.borrow().c.is_none() {
            let prev = acc.stack.last().unwrap();
            prev.node.borrow_mut().c.as_mut().unwrap().insert(top.ch, acc.nodes.pop().unwrap());
        }
    }

    fn parse_back(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
        let cnt = if ch == BACK { 1 } else { ch.to_digit(10).unwrap() as usize - 1 };
        for _ in 0..cnt { acc.stack.pop(); }
        Some(parse_back)
    }

    fn parse_reference(acc: &mut ReduceResults, rch: char) {
        let is_index = rch == REF_REL;
        let mut ref_str = String::new();
        acc.nodes.pop();
        fn inner(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
            if ch == EOR || (!acc.root.info.is_case_aware && !ch.is_ascii_digit()) {
                let idx = usize::from_str_radix(&ref_str, acc.parser as usize).unwrap_or(0);
                let target = if is_index { ref_index[idx] } else { idx };
                let prev = acc.stack.pop().unwrap();
                let parent = &acc.stack.last().unwrap().node;
                parent.borrow_mut().c.as_mut().unwrap().insert(prev.ch, acc.nodes[target].clone());
                return None;
            }
            ref_str.push(ch);
            Some(inner)
        }
        acc.parser = Some(inner);
    }

    fn parse_escape(acc: &mut ReduceResults, _: char) -> Option<fn(&mut ReduceResults, char)> {
        let mut prev = None;
        fn inner(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
            if let Some(p) = prev.take() {
                let mut s = p.to_string(); s.push(ch);
                let real = special_char_map().get(&s).cloned().unwrap_or(ch);
                parse_char(acc, real);
                None
            } else if ch == ESCAPE {
                prev = Some(ch);
                Some(inner)
            } else {
                parse_char(acc, ch);
                None
            }
        }
        Some(inner)
    }

    fn parse_comment(acc: &mut ReduceResults, _: char) -> Option<fn(&mut ReduceResults, char)> {
        let mut escape = false;
        fn inner(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
            if escape { escape = false; return Some(inner); }
            if ch == ESCAPE { escape = true; return Some(inner); }
            if ch == INLINE_DATA_COMMENT_LINE { return None; }
            Some(inner)
        }
        Some(inner)
    }

    fn parse_ref_index(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
        let mut buf = String::new();
        fn start(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
            if ch == REF_INDEX_BEGIN {
                buf.push(ch);
                return Some(inner);
            }
            if spaces.contains_key(&ch) { return Some(start); }
            None
        }
        fn inner(acc: &mut ReduceResults, ch: char) -> Option<fn(&mut ReduceResults, char)> {
            buf.push(ch);
            if ch == REF_INDEX_END {
                buf.retain(|c| c!='[' && c!=']' && !c.is_whitespace());
                ref_index = buf.split(',')
                    .map(|s| usize::from_str_radix(s, radix).unwrap())
                    .collect();
                return None;
            }
            Some(inner)
        }
        Some(start)
    }

    // Run parsing
    for line in lines {
        for ch in line.chars() {
            parser_main(&mut acc, ch, &special_map, &numbers, &spaces);
        }
    }

    acc.root
}
