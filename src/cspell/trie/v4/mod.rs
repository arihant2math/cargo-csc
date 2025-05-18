use std::collections::{HashSet, HashMap, BTreeMap};
use once_cell::sync::Lazy;

mod constants;
use constants::*;

/// Begin of Reference Index
pub const REF_INDEX_BEGIN: char = '[';

/// End of Reference Index
pub const REF_INDEX_END: char = ']';

/// Inline Data Comment Line
pub const INLINE_DATA_COMMENT_LINE: char = '%';

fn string_to_char_set(s: &str) -> HashSet<char> {
    s.chars().collect()
}

const SPECIAL_CHARACTERS_MAP: &[(char, &str)] = &[
    ('\n', "\\n"),
    ('\r', "\\r"),
    ('\\', "\\\\"),
];

fn string_to_char_map(map: &[(char, &str)]) -> HashMap<char, char> {
    map.iter().map(|&(k, v)| (k, v.chars().next().unwrap())).collect()
}

pub static SPECIAL_CHARACTER_MAP: Lazy<HashMap<char, char>> = Lazy::new(|| {
    string_to_char_map(SPECIAL_CHARACTERS_MAP)
});

pub static CHARACTER_MAP: Lazy<HashMap<char, char>> = Lazy::new(|| {
    string_to_char_map(&SPECIAL_CHARACTERS_MAP.iter().map(|&(k, v)| (v.chars().next().unwrap(), k)).collect::<Vec<_>>())
});

pub static SPECIAL_PREFIX: Lazy<HashSet<char>> = Lazy::new(|| {
    string_to_char_set("~!")
});

pub const WORDS_PER_LINE: usize = 20;

pub const DATA: &str = "__DATA__";

pub static SPECIAL_CHARACTERS: Lazy<HashSet<char>> = Lazy::new(|| {
    string_to_char_set(&[
        EOW,
        BACK,
        EOL,
        REF,
        REF_REL,
        EOR,
        ESCAPE,
        LF,
        REF_INDEX_BEGIN,
        REF_INDEX_END,
        INLINE_DATA_COMMENT_LINE,
    ]
    .iter()
    .collect::<String>()
    + "0123456789`~!@#$%^&*()_-+=[]{};:'\"<>,./?\\|")
});

pub struct ReferenceMap {
    /**
     * An array of references to nodes.
     * The most frequently referenced is first in the list.
     * A node must be referenced by other nodes to be included.
     */
    pub ref_counts: Vec<(TrieNode, usize)>,
}

pub fn build_reference_map(root: &TrieRoot, base: usize) -> ReferenceMap {
    struct Ref {
        count: usize, // count
        node_number: usize, // node number
    }

    let mut ref_count: BTreeMap<TrieNode, Ref> = BTreeMap::new();
    let mut node_count = 0;

    fn walk(node: &TrieNode, ref_count: &mut BTreeMap<TrieNode, Ref>, node_count: &mut usize) {
        if let Some(ref_entry) = ref_count.get_mut(node) {
            ref_entry.count += 1;
            return;
        }
        ref_count.insert(
            node.clone(),
            Ref {
                count: 1,
                node_number: *node_count,
            },
        );
        *node_count += 1;
        if let Some(children) = &node.children {
            for child in children.values() {
                walk(child, ref_count, node_count);
            }
        }
    }

    walk(root, &mut ref_count, &mut node_count);

    let mut ref_count_and_node: Vec<_> = ref_count
        .into_iter()
        .filter(|(_, ref_entry)| ref_entry.count >= 2)
        .collect();

    ref_count_and_node.sort_by(|a, b| {
        b.1.count.cmp(&a.1.count).then_with(|| a.1.node_number.cmp(&b.1.node_number))
    });

    let mut adj = 0;
    let base_log_scale = 1.0 / (base as f64).ln();
    let refs = ref_count_and_node
        .into_iter()
        .enumerate()
        .filter_map(|(idx, (node, ref_entry))| {
            let i = idx as f64 - adj as f64;
            let chars_idx = (i.ln() * base_log_scale).ceil() as usize;
            let chars_node = (ref_entry.node_number as f64).ln() * base_log_scale;
            let savings = ref_entry.count as f64 * (chars_node - chars_idx as f64) - chars_idx as f64;
            if savings > 0.0 {
                Some((node, ref_entry.count))
            } else {
                adj += 1;
                None
            }
        })
        .collect();

    ReferenceMap { ref_counts: refs }
}

pub struct Stack {
    pub node: TrieNode,
    pub s: String,
}

pub struct ReduceResults {
    pub stack: Vec<Stack>,
    pub nodes: Vec<TrieNode>,
    pub root: TrieRoot,
    pub parser: Option<Reducer>,
}

pub type Reducer = fn(acc: ReduceResults, s: &str) -> ReduceResults;

pub fn import_trie(lines: impl IntoIterator<Item = String>) -> TrieRoot {
    let mut radix = 10;
    let comment = regex::Regex::new(r"^\s*#").unwrap();
    let mut iter = lines.into_iter();

    fn parse_header_rows(header_rows: &[String]) -> usize {
        let header = header_rows.join("\n");
        let header_reg = regex::Regex::new(r"^TrieXv[34]\nbase=(\d+)$").unwrap();
        if !header_reg.is_match(&header) {
            panic!("Unknown file format");
        }
        header_reg
            .captures(&header)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().parse::<usize>().unwrap())
            .unwrap()
    }

    fn read_header(iter: &mut impl Iterator<Item = String>, comment: &regex::Regex) -> usize {
        let mut header_rows = Vec::new();
        for value in iter {
            let line = value.trim();
            if line.is_empty() || comment.is_match(line) {
                continue;
            }
            if line == DATA {
                break;
            }
            header_rows.push(line.to_string());
        }
        parse_header_rows(&header_rows)
    }

    radix = read_header(&mut iter, &comment);

    parse_stream(radix, iter)
}

pub static NUMBERS_SET: Lazy<HashSet<char>> = Lazy::new(|| string_to_char_set("0123456789"));

use std::collections::HashMap;

pub fn parse_stream(radix: usize, iter: impl IntoIterator<Item = String>) -> TrieRoot {
    let eow = TrieNode { f: Some(1), c: None, n: None };
    let mut ref_index: Vec<usize> = Vec::new();
    let root = TrieRoot::new();

    fn parse_reference(mut acc: ReduceResults, s: &str, is_index_ref: bool, radix: usize, ref_index: &[usize]) -> ReduceResults {
        let mut ref_str = String::new();

        let parser = move |mut acc: ReduceResults, s: &str| {
            if s == EOR || (radix == 10 && !NUMBERS_SET.contains(&s.chars().next().unwrap())) {
                let r = usize::from_str_radix(&ref_str, radix).unwrap();
                let top = acc.stack.last_mut().unwrap();
                let parent = acc.stack.get_mut(acc.stack.len() - 2).unwrap().node.clone();
                let n = if is_index_ref { ref_index[r] } else { r };
                if let Some(children) = &mut parent.c {
                    children.insert(top.s.clone(), acc.nodes[n].clone());
                }
                acc.parser = None;
                return if s == EOR { acc } else { parser_main(acc, s) };
            }
            ref_str.push_str(s);
            acc
        };

        acc.nodes.pop();
        acc.parser = Some(Box::new(parser));
        acc
    }

    fn parse_escape_character(mut acc: ReduceResults, _: &str) -> ReduceResults {
        let mut prev = None;

        let parser = move |mut acc: ReduceResults, s: &str| {
            if let Some(p) = prev {
                let combined = format!("{}{}", p, s);
                let mapped = CHARACTER_MAP.get(&combined).cloned().unwrap_or_else(|| s.to_string());
                return parse_character(acc, &mapped);
            }
            if s == ESCAPE {
                prev = Some(s.to_string());
                return acc;
            }
            parse_character(acc, s)
        };

        acc.parser = Some(Box::new(parser));
        acc
    }

    fn parse_comment(mut acc: ReduceResults, end_of_comment: &str) -> ReduceResults {
        let mut is_escaped = false;

        let parser = move |mut acc: ReduceResults, s: &str| {
            if is_escaped {
                is_escaped = false;
                return acc;
            }
            if s == ESCAPE {
                is_escaped = true;
                return acc;
            }
            if s == end_of_comment {
                acc.parser = None;
                return acc;
            }
            acc
        };

        acc.parser = Some(Box::new(parser));
        acc
    }

    fn parse_character(mut acc: ReduceResults, s: &str) -> ReduceResults {
        let top = acc.stack.last_mut().unwrap();
        let node = &mut top.node;
        let children = node.c.get_or_insert_with(HashMap::new);
        let new_node = TrieNode { f: None, c: None, n: Some(acc.nodes.len()) };
        children.insert(s.to_string(), new_node.clone());
        acc.stack.push(Stack { node: new_node.clone(), s: s.to_string() });
        acc.nodes.push(new_node);
        acc
    }

    fn parse_eow(mut acc: ReduceResults, _: &str) -> ReduceResults {
        let top = acc.stack.pop().unwrap();
        let node = &mut top.node;
        node.f = Some(FLAG_WORD);
        if node.c.is_none() {
            let parent = acc.stack.last_mut().unwrap().node.clone();
            if let Some(children) = &mut parent.c {
                children.insert(top.s, eow.clone());
            }
            acc.nodes.pop();
        }
        acc.parser = Some(Box::new(parse_back));
        acc
    }

    fn parse_back(mut acc: ReduceResults, s: &str) -> ReduceResults {
        if !BACK.contains(&s.chars().next().unwrap()) {
            return parser_main(acc, s);
        }
        let mut n = if s == BACK { 1 } else { s.parse::<usize>().unwrap() - 1 };
        while n > 0 {
            acc.stack.pop();
            n -= 1;
        }
        acc.parser = Some(Box::new(parse_back));
        acc
    }

    fn parser_main(mut acc: ReduceResults, s: &str) -> ReduceResults {
        let parser = acc.parser.take().unwrap_or_else(|| Box::new(parse_character));
        parser(acc, s)
    }

    let mut acc = ReduceResults {
        nodes: vec![root.clone()],
        root: root.clone(),
        stack: vec![Stack { node: root.clone(), s: String::new() }],
        parser: Some(Box::new(parse_reference_index)),
    };

    for line in iter {
        for ch in line.chars() {
            acc = parser_main(acc, &ch.to_string());
        }
    }

    root
}

