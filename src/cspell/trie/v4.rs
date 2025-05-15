//! Trie file format v4
//!
//! Trie format v4 is very similar to v3. The v4 reader can even read v3 files.
//! The motivation behind v4 is to reduce the cost of storing `.trie` files in git.
//! When a word is added in v3, nearly the entire file is changed due to the absolute
//! references. V4 adds an index sorted by the most frequently used reference to the least.
//! Because git diff is line based, it is important to add line breaks at logical points.
//! V3 added line breaks just to make sure the lines were not too long, V4 takes a different
//! approach. Line breaks are added at two distinct points. First, at the start of each two
//! letter prefix and second after approximately 50 words have been emitted.
//!
//! To improve readability and git diff, at the beginning of each two letter prefix,
//! a comment is emitted.

// import { opAppend, opConcatMap, opFilter, pipe, reduce } from '@cspell/cspell-pipe/sync';
//
// import { trieNodeToRoot } from '../TrieNode/trie-util.js';
// import type { TrieNode, TrieRoot } from '../TrieNode/TrieNode.js';
// import { FLAG_WORD } from '../TrieNode/TrieNode.js';
// import { bufferLines } from '../utils/bufferLines.js';

use std::cell::RefCell;
use crate::{HashMap, HashSet};
use std::rc::Rc;

// export interface TrieNode {
//     f?: number | undefined; // flags
//     c?: ChildMap | undefined;
// }
pub struct CspellTrieNode {
    f: bool,
    c: Option<HashMap<char, Rc<RefCell<CspellTrieNode>>>>,
}

pub struct CspellTrieRoot(CspellTrieNode);

impl CspellTrieRoot {
    pub fn contains(&self, word: &str) -> bool {
        let mut current_node = &self.0;
        for c in word.chars() {
            match current_node.c.as_ref().and_then(|c| c.get(&c)) {
                Some(node) => current_node = node,
                None => return false,
            }
        }
        current_node.f
    }

    pub fn collect_words(
        &self,
        node: &CspellTrieNode,
        prefix: String,
        words: &mut Vec<String>,
    ) {
        if node.f {
            words.push(prefix.clone());
        }

        if let Some(ref children) = node.c {
            for (c, child_node) in children {
                let mut new_prefix = prefix.clone();
                new_prefix.push(*c);
                self.collect_words(child_node.borrow().as_ref(), new_prefix, words);
            }
        }
    }

    pub fn to_vec(&self) -> Vec<String> {
        let mut words = Vec::new();
        self.collect_words(&self.0, String::new(), &mut words);
        words
    }
}

fn string_to_char_set(values: &str) -> std::collections::HashSet<char> {
    let mut set = std::collections::HashSet::new();
    for c in values.chars() {
        set.insert(c);
    }
    set
}

const REF_INDEX_BEGIN: &str = '[';
const REF_INDEX_END: &str = ']';
const INLINE_DATA_COMMENT_LINE: &str = '/';

/// End of word
const EOW: &str = '$';

/// Move up the tree
const BACK = '<';

/// End of Line (ignored)
const EOL = '\n';

/// Line Feed (ignored)
const LF = '\r';

/// Start of Absolute Reference
const REF = '#';

/// Start indexed of Reference
const REF_REL = '@';

/// End of Reference
const EOR = ';';

/// Escape the next character
const ESCAPE = '\\';

fn special_character_map() -> HashSet<char> {
    let mut s = format!("{EOW}{BACK}{EOL}{REF}{REF_REL}{EOR}{ESCAPE}{LF}{REF_INDEX_BEGIN}{REF_INDEX_END}{INLINE_DATA_COMMENT_LINE}");
    s += "0123456789";
    s += "`~!@#$%^&*()_-+=[]{};:'\"<>,./?\\|";
    string_to_char_set(&s)
}

// const SPECIAL_CHARACTERS_MAP = [
//     ['\n', '\\n'],
//     ['\r', '\\r'],
//     ['\\', '\\\\'],
// ] as const;

fn special_character_vec() -> Vec<(char, String)> {
    let mut s = vec![
        ('\n', "\\n".to_string()),
        ('\r', "\\r".to_string()),
        ('\\', "\\\\".to_string()),
    ];
    s
}

// const specialCharacterMap = stringToCharMap(SPECIAL_CHARACTERS_MAP);
fn special_character_map() -> Vec<(char, String)> {
    let mut s = vec![
        ('\n', "\\n".to_string()),
        ('\r', "\\r".to_string()),
        ('\\', "\\\\".to_string()),
    ];
    s
}
// const characterMap = stringToCharMap(SPECIAL_CHARACTERS_MAP.map((a) => [a[1], a[0]]));
fn character_map() -> Vec<(String, char)> {
    let mut s = vec![
        ("\\n".to_string(), '\n'),
        ("\\r".to_string(), '\r'),
        ("\\\\".to_string(), '\\'),
    ];
    s
}
// const specialPrefix = stringToCharSet('~!');
fn special_prefix() -> HashSet<char> {
    string_to_char_set("~!")
}
// const WORDS_PER_LINE = 20;
const WORDS_PER_LINE: usize = 20;
// export const DATA = '__DATA__';
const DATA: &str = "__DATA__";
// function generateHeader(base: number, comment: string): string {
//     const comments = comment
//         .split('\n')
//         .map((a) => '# ' + a.trimEnd())
//         .join('\n');
//
//     return `\
// #!/usr/bin/env cspell-trie reader
// TrieXv4
// base=${base}
// ${comments}
// # Data:
// ${DATA}
// `;
// }
fn generate_header(base: usize, comment: &str) -> String {
    let comments = comment
        .lines()
        .map(|a| format!("# {}", a.trim_end()))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env cspell-trie reader
TrieXv4
base={}
{}
# Data:
{}
"#,
        base, comments, DATA
    )
}

// export interface ExportOptions {
//     base?: number;
//     comment?: string;
//     /**
//      * This will reduce the size of the `.trie` file by removing references to short suffixes.
//      * But it does increase the size of the trie when loaded into memory.
//      */
//     optimizeSimpleReferences?: boolean;
// }

pub struct ExportOptions {
    base: usize,
    comment: String,
    optimize_simple_references: bool,
}

// /**
//  * Serialize a TrieRoot.
//  */
// export function serializeTrie(root: TrieRoot, options: ExportOptions | number = 16): Iterable<string> {
//     options = typeof options === 'number' ? { base: options } : options;
//     const { base = 10, comment = '' } = options;
//     const radix = base > 36 ? 36 : base < 10 ? 10 : base;
//     const cache = new Map<TrieNode, number>();
//     const refMap = buildReferenceMap(root, base);
//     const nodeToIndexMap = new Map(refMap.refCounts.map(([node], index) => [node, index]));
//     let count = 0;
//     const backBuffer = { last: '', count: 0, words: 0, eol: false };
//     const wordChars: string[] = [];
//
//     function ref(n: number, idx: number | undefined): string {
//         const r = idx === undefined || n < idx ? REF + n.toString(radix) : REF_REL + idx.toString(radix);
//         return radix === 10 ? r : r + ';';
//     }
//
//     function escape(s: string): string {
//         return s in specialCharacters ? ESCAPE + (specialCharacterMap[s] || s) : s;
//     }
//
//     function* flush() {
//         while (backBuffer.count) {
//             const n = Math.min(9, backBuffer.count);
//             yield n > 1 ? backBuffer.last + n : backBuffer.last;
//             backBuffer.last = BACK;
//             backBuffer.count -= n;
//         }
//         if (backBuffer.eol) {
//             yield EOL;
//             backBuffer.eol = false;
//             backBuffer.words = 0;
//         }
//     }
//
//     function* emit(s: string): Generator<string> {
//         switch (s) {
//             case EOW: {
//                 yield* flush();
//                 backBuffer.last = EOW;
//                 backBuffer.count = 0;
//                 backBuffer.words++;
//                 break;
//             }
//             case BACK: {
//                 backBuffer.count++;
//                 break;
//             }
//             case EOL: {
//                 backBuffer.eol = true;
//                 break;
//             }
//             default: {
//                 if (backBuffer.words >= WORDS_PER_LINE) {
//                     backBuffer.eol = true;
//                 }
//                 yield* flush();
//                 if (s.startsWith(REF) || s.startsWith(REF_REL)) {
//                     backBuffer.words++;
//                 }
//                 yield s;
//             }
//         }
//     }
//
//     const comment_begin = `${EOL}${INLINE_DATA_COMMENT_LINE}* `;
//     const comment_end = ` *${INLINE_DATA_COMMENT_LINE}${EOL}`;
//
//     function* walk(node: TrieNode, depth: number): Generator<string> {
//         const nodeNumber = cache.get(node);
//         const refIndex = nodeToIndexMap.get(node);
//         if (nodeNumber !== undefined) {
//             yield* emit(ref(nodeNumber, refIndex));
//             return;
//         }
//         if (node.c) {
//             if (depth > 0 && depth <= 2) {
//                 const chars = wordChars.slice(0, depth).map(escape).join('');
//                 yield* emit(comment_begin + chars + comment_end);
//             }
//             cache.set(node, count++);
//             const c = Object.entries(node.c).sort((a, b) => (a[0] < b[0] ? -1 : 1));
//             for (const [s, n] of c) {
//                 wordChars[depth] = s;
//                 yield* emit(escape(s));
//                 yield* walk(n, depth + 1);
//                 yield* emit(BACK);
//                 if (depth === 0) yield* emit(EOL);
//             }
//         }
//         // Output EOW after children so it can be optimized on read
//         if (node.f) {
//             yield* emit(EOW);
//         }
//         if (depth === 2 || (depth === 3 && wordChars[0] in specialPrefix)) {
//             yield* emit(EOL);
//         }
//     }
//
//     function* serialize(node: TrieNode): Generator<string> {
//         yield* walk(node, 0);
//         yield* flush();
//     }
//
//     const lines = [...bufferLines(serialize(root), 1000, '')];
//
//     const resolvedReferences = refMap.refCounts.map(([node]) => cache.get(node) || 0);
//
//     // const r = refMap.refCounts.slice(0, 200).map(([node, c]) => ({ n: cache.get(node) || 0, c }));
//     // console.log('First 100: %o \n %o', r.slice(0, 100), r.slice(100, 200));
//
//     const reference =
//         '[\n' +
//         resolvedReferences
//             .map((n) => n.toString(radix))
//             .join(',')
//             .replaceAll(/.{110,130}[,]/g, '$&\n') +
//         '\n]\n';
//
//     return pipe([generateHeader(radix, comment), reference], opAppend(lines));
// }
//
// interface ReferenceMap {
//     /**
//      * An array of references to nodes.
//      * The most frequently referenced is first in the list.
//      * A node must be reference by other nodes to be included.
//      */
//     refCounts: (readonly [TrieNode, number])[];
// }
struct ReferenceMap {
    ref_counts: Vec<(Rc<RefCell<CspellTrieNode>>, usize)>,
}
//
// function buildReferenceMap(root: TrieRoot, base: number): ReferenceMap {
//     interface Ref {
//         c: number; // count
//         n: number; // node number;
//     }
//     const refCount = new Map<TrieNode, Ref>();
//     let nodeCount = 0;
//
//     function walk(node: TrieNode) {
//         const ref = refCount.get(node);
//         if (ref) {
//             ref.c++;
//             return;
//         }
//         refCount.set(node, { c: 1, n: nodeCount++ });
//         if (!node.c) return;
//         for (const child of Object.values(node.c)) {
//             walk(child);
//         }
//     }
//
//     walk(root);
//     // sorted highest to lowest
//     const refCountAndNode = [
//         ...pipe(
//             refCount,
//             opFilter(([_, ref]) => ref.c >= 2),
//         ),
//     ].sort((a, b) => b[1].c - a[1].c || a[1].n - b[1].n);
//
//     let adj = 0;
//     const baseLogScale = 1 / Math.log(base);
//     const refs = refCountAndNode
//         .filter(([_, ref], idx) => {
//             const i = idx - adj;
//             const charsIdx = Math.ceil(Math.log(i) * baseLogScale);
//             const charsNode = Math.ceil(Math.log(ref.n) * baseLogScale);
//             const savings = ref.c * (charsNode - charsIdx) - charsIdx;
//             const keep = savings > 0;
//             adj += keep ? 0 : 1;
//             return keep;
//         })
//         .map(([n, ref]) => [n, ref.c] as const);
//
//     return { refCounts: refs };
// }
//
// interface Stack {
//     node: TrieNode;
//     s: string;
// }
struct Stack {
    node: Rc<RefCell<CspellTrieNode>>,
    s: String,
}

// interface ReduceResults {
//     stack: Stack[];
//     nodes: TrieNode[];
//     root: TrieRoot;
//     parser: Reducer | undefined;
// }

struct ReduceResults {
    stack: Vec<Stack>,
    nodes: Vec<Rc<RefCell<CspellTrieNode>>>,
    root: TrieRoot,
    parser: Option<Box<dyn Fn(&mut ReduceResults, &str) -> ReduceResults>>,
}

// type Reducer = (acc: ReduceResults, s: string) => ReduceResults;
type Reducer = fn(&mut ReduceResults, &str) -> ReduceResults;

// export function importTrie(linesX: Iterable<string> | string): TrieRoot {
fn import_trie(lines_x: impl IntoIterator<Item=String>) -> CspellTrieNode {
    //     linesX = typeof linesX === 'string' ? linesX.split(/^/m) : linesX;
    //     let radix = 10;
    let radix = 10;
    //     const comment = /^\s*#/;
    let comment = regex::Regex::new(r"^\s*#").unwrap();
    //     const iter = tapIterable(
    //         pipe(
    //             linesX,
    //             opConcatMap((a) => a.split(/^/m)),
    //         ),
    //     );
    let iter = lines_x.into_iter();
    // TODO
    //
    //     function parseHeaderRows(headerRows: string[]) {
    //         const header = headerRows.slice(0, 2).join('\n');
    //         const headerReg = /^TrieXv[34]\nbase=(\d+)$/;
    //         /* istanbul ignore if */
    //         if (!headerReg.test(header)) throw new Error('Unknown file format');
    //         radix = Number.parseInt(header.replace(headerReg, '$1'), 10);
    //     }
    fn parse_header_rows(header_rows: Vec<String>) {
        let header = header_rows.iter().take(2).collect::<Vec<_>>().join("\n");
        let header_reg = regex::Regex::new(r"^TrieXv[34]\nbase=(\d+)$").unwrap();
        if !header_reg.is_match(&header) {
            panic!("Unknown file format");
        }
        radix = header.replace(header_reg.as_str(), "$1").parse::<usize>().unwrap();
    }
    //     function readHeader(iter: Iterable<string>) {
    //         const headerRows: string[] = [];
    //         for (const value of iter) {
    //             const line = value.trim();
    //             if (!line || comment.test(line)) continue;
    //             if (line === DATA) break;
    //             headerRows.push(line);
    //         }
    //         parseHeaderRows(headerRows);
    //     }
    fn read_header(iter: &mut dyn Iterator<Item=String>) {
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
        parse_header_rows(header_rows);
    }
    //     readHeader(iter);
    read_header(iter);
    //     const root = parseStream(radix, iter);
    let root = parse_stream(radix, iter);
    //     return root;
    root
}
//
// const numbersSet = stringToCharSet('0123456789');
fn numbers_set() -> HashSet<char> {
    string_to_char_set("0123456789")
}
//
// function parseStream(radix: number, iter: Iterable<string>): TrieRoot {
fn parse_stream(radix: usize, iter: impl IntoIterator<Item=String>) -> CspellTrieRoot {
    //     const eow: TrieNode = Object.freeze({ f: 1 });
    let eow = CspellTrieNode {
        f: true,
        c: None,
    };

    //     let refIndex: number[] = [];
    let ref_index: Vec<usize> = Vec::new();
    //     const root: TrieRoot = trieNodeToRoot({}, {});
    let root = CspellTrieNode {
        f: false,
        c: None,
    };
    //     function parseReference(acc: ReduceResults, s: string): ReduceResults {
    //         const isIndexRef = s === REF_REL;
    //         let ref = '';
    //
    //         function parser(acc: ReduceResults, s: string): ReduceResults {
    //             if (s === EOR || (radix === 10 && !(s in numbersSet))) {
    //                 const { root, nodes, stack } = acc;
    //                 const r = Number.parseInt(ref, radix);
    //                 const top = stack[stack.length - 1];
    //                 const p = stack[stack.length - 2].node;
    //                 const n = isIndexRef ? refIndex[r] : r;
    //                 p.c && (p.c[top.s] = nodes[n]);
    //                 const rr = { root, nodes, stack, parser: undefined };
    //                 return s === EOR ? rr : parserMain(rr, s);
    //             }
    //             ref = ref + s;
    //             return acc;
    //         }
    //
    //         const { nodes } = acc;
    //         nodes.pop();
    //         return { ...acc, nodes, parser };
    //     }
    fn parse_reference(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        let is_index_ref = s == REF_REL;
        let mut ref_ = String::new();

        fn parser(acc: &mut ReduceResults, s: &str) -> ReduceResults {
            if s == EOR || (radix == 10 && !numbers_set().contains(s.chars().next().unwrap())) {
                let root = acc.root.clone();
                let nodes = acc.nodes.clone();
                let stack = acc.stack.clone();
                let top = stack.last().unwrap();
                let p = stack.get(stack.len() - 2).unwrap().node.clone();
                let n = if is_index_ref {
                    ref_index[ref_.parse::<usize>().unwrap()]
                } else {
                    ref_.parse::<usize>().unwrap()
                };
                p.c.as_mut().unwrap().insert(top.s.chars().next().unwrap(), nodes[n].clone());
                let rr = if s == EOR {
                    ReduceResults {
                        root,
                        nodes,
                        stack,
                        parser: None,
                    }
                } else {
                    parser_main(acc, s)
                };
                return rr;
            }
            ref_.push_str(s);
            acc
        }

        let nodes = acc.nodes.clone();
        nodes.pop();
        ReduceResults {
            root: acc.root.clone(),
            nodes,
            parser: Some(Box::new(parser)),
        }
    }
    //
    //     function parseEscapeCharacter(acc: ReduceResults, _: string): ReduceResults {
    //         let prev = '';
    //         const parser = function (acc: ReduceResults, s: string): ReduceResults {
    //             if (prev) {
    //                 s = characterMap[prev + s] || s;
    //                 return parseCharacter({ ...acc, parser: undefined }, s);
    //             }
    //             if (s === ESCAPE) {
    //                 prev = s;
    //                 return acc;
    //             }
    //             return parseCharacter({ ...acc, parser: undefined }, s);
    //         };
    //         return { ...acc, parser };
    //     }
    fn parse_escape_character(acc: &mut ReduceResults, _: &str) -> ReduceResults {
        let mut prev = String::new();
        let parser = |acc: &mut ReduceResults, s: &str| {
            if !prev.is_empty() {
                let s = character_map()
                    .iter()
                    .find(|(k, _)| *k == prev + s)
                    .map(|(_, v)| v)
                    .unwrap_or(s);
                return parse_character(acc, s);
            }
            if s == ESCAPE {
                prev = s.to_string();
                return acc;
            }
            parse_character(acc, s)
        };
        ReduceResults {
            root: acc.root.clone(),
            nodes: acc.nodes.clone(),
            stack: acc.stack.clone(),
            parser: Some(Box::new(parser)),
        }
    }
    //
    //     function parseComment(acc: ReduceResults, s: string): ReduceResults {
    //         const endOfComment = s;
    //         let isEscaped = false;
    //
    //         function parser(acc: ReduceResults, s: string): ReduceResults {
    //             if (isEscaped) {
    //                 isEscaped = false;
    //                 return acc;
    //             }
    //             if (s === ESCAPE) {
    //                 isEscaped = true;
    //                 return acc;
    //             }
    //             if (s === endOfComment) {
    //                 return { ...acc, parser: undefined };
    //             }
    //             return acc;
    //         }
    //         return { ...acc, parser };
    //     }
    fn parse_comment(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        let end_of_comment = s.to_string();
        let mut is_escaped = false;

        let parser = |acc: &mut ReduceResults, s: &str| {
            if is_escaped {
                is_escaped = false;
                return acc;
            }
            if s == ESCAPE {
                is_escaped = true;
                return acc;
            }
            if s == end_of_comment {
                return ReduceResults {
                    root: acc.root.clone(),
                    nodes: acc.nodes.clone(),
                    stack: acc.stack.clone(),
                    parser: None,
                };
            }
            acc
        };
        ReduceResults {
            root: acc.root.clone(),
            nodes: acc.nodes.clone(),
            stack: acc.stack.clone(),
            parser: Some(Box::new(parser)),
        }
    }
    //
    //     function parseCharacter(acc: ReduceResults, s: string): ReduceResults {
    //         const parser = undefined;
    //         const { root, nodes, stack } = acc;
    //         const top = stack[stack.length - 1];
    //         const node = top.node;
    //         const c = node.c ?? Object.create(null);
    //         const n = { f: undefined, c: undefined, n: nodes.length };
    //         c[s] = n;
    //         node.c = c;
    //         stack.push({ node: n, s });
    //         nodes.push(n);
    //         return { root, nodes, stack, parser };
    //     }
    fn parse_character(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        let parser = None;
        let root = acc.root.clone();
        let nodes = acc.nodes.clone();
        let stack = acc.stack.clone();
        let top = stack.last().unwrap();
        let node = top.node.clone();
        let c = node.c.clone().unwrap_or_else(|| {
            std::collections::HashMap::new()
        });
        let n = CspellTrieNode {
            f: false,
            c: None,
        };
        c.insert(s.chars().next().unwrap(), Rc::new(RefCell::new(n)));
        node.c = Some(c);
        stack.push(Stack {
            node: Rc::new(RefCell::new(n)),
            s: s.to_string(),
        });
        nodes.push(Rc::new(RefCell::new(n)));
        ReduceResults {
            root,
            nodes,
            stack,
            parser,
        }
    }
    //
    //     function parseEOW(acc: ReduceResults, _: string): ReduceResults {
    //         const parser = parseBack;
    //         const { root, nodes, stack } = acc;
    //         const top = stack[stack.length - 1];
    //         const node = top.node;
    //         node.f = FLAG_WORD;
    //         if (!node.c) {
    //             top.node = eow;
    //             const p = stack[stack.length - 2].node;
    //             p.c && (p.c[top.s] = eow);
    //             nodes.pop();
    //         }
    //         stack.pop();
    //         return { root, nodes, stack, parser };
    //     }
    fn parse_eow(acc: &mut ReduceResults, _: &str) -> ReduceResults {
        let parser = Some(Box::new(parse_back));
        let root = acc.root.clone();
        let nodes = acc.nodes.clone();
        let stack = acc.stack.clone();
        let top = stack.last().unwrap();
        let node = top.node.clone();
        node.f = true;
        if node.c.is_none() {
            top.node = Rc::new(RefCell::new(eow));
            let p = stack.get(stack.len() - 2).unwrap().node.clone();
            p.c.as_mut().unwrap().insert(top.s.chars().next().unwrap(), Rc::new(RefCell::new(eow)));
            nodes.pop();
        }
        stack.pop();
        ReduceResults {
            root,
            nodes,
            stack,
            parser,
        }
    }
    //
    //     const charactersBack = stringToCharSet(BACK + '23456789');
    let characters_back = string_to_char_set(&format!("{}23456789", BACK));
    //     function parseBack(acc: ReduceResults, s: string): ReduceResults {
    //         if (!(s in charactersBack)) {
    //             return parserMain({ ...acc, parser: undefined }, s);
    //         }
    //         let n = s === BACK ? 1 : Number.parseInt(s, 10) - 1;
    //         const { stack } = acc;
    //         while (n-- > 0) {
    //             stack.pop();
    //         }
    //         return { ...acc, parser: parseBack };
    //     }
    fn parse_back(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        if !special_character_map().contains(&s.chars().next().unwrap()) {
            return parser_main(acc, s);
        }
        let mut n = if s == BACK { 1 } else { s.parse::<usize>().unwrap() - 1 };
        let stack = acc.stack.clone();
        while n > 0 {
            stack.pop();
            n -= 1;
        }
        ReduceResults {
            root: acc.root.clone(),
            nodes: acc.nodes.clone(),
            stack,
            parser: Some(Box::new(parse_back)),
        }
    }
    //
    //     function parseIgnore(acc: ReduceResults, _: string): ReduceResults {
    //         return acc;
    //     }
    fn parse_ignore(acc: &mut ReduceResults, _: &str) -> ReduceResults {
        acc.clone()
    }
    //
    //     const parsers = createStringLookupMap([
    //         [EOW, parseEOW],
    //         [BACK, parseBack],
    //         [REF, parseReference],
    //         [REF_REL, parseReference],
    //         [ESCAPE, parseEscapeCharacter],
    //         [EOL, parseIgnore],
    //         [LF, parseIgnore],
    //         [INLINE_DATA_COMMENT_LINE, parseComment],
    //     ]);
    let parsers = HashMap::new();
    parsers.insert(EOW.to_string(), parse_eow);
    parsers.insert(BACK.to_string(), parse_back);
    parsers.insert(REF.to_string(), parse_reference);
    parsers.insert(REF_REL.to_string(), parse_reference);
    parsers.insert(ESCAPE.to_string(), parse_escape_character);
    parsers.insert(EOL.to_string(), parse_ignore);
    parsers.insert(LF.to_string(), parse_ignore);
    parsers.insert(INLINE_DATA_COMMENT_LINE.to_string(), parse_comment);

    //     function parserMain(acc: ReduceResults, s: string): ReduceResults {
    //         const parser = acc.parser ?? parsers[s] ?? parseCharacter;
    //         return parser(acc, s);
    //     }
    fn parser_main(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        let parser = acc.parser.clone().unwrap_or_else(|| {
            parsers.get(s).unwrap_or(&parse_character)
        });
        parser(acc, s)
    }

    //     const charsetSpaces = stringToCharSet(' \r\n\t');
    let charset_spaces = string_to_char_set(" \r\n\t");

    //     function parseReferenceIndex(acc: ReduceResults, s: string): ReduceResults {
    //         let json = '';
    //
    //         function parserStart(acc: ReduceResults, s: string): ReduceResults {
    //             if (s === REF_INDEX_BEGIN) {
    //                 json = json + s;
    //                 return { ...acc, parser };
    //             }
    //             if (s in charsetSpaces) {
    //                 return acc;
    //             }
    //             // A Reference Index was not found.
    //             return parserMain({ ...acc, parser: undefined }, s);
    //         }
    //
    //         function parser(acc: ReduceResults, s: string): ReduceResults {
    //             json = json + s;
    //             if (s === REF_INDEX_END) {
    //                 refIndex = json
    //                     .replaceAll(/[\s[\]]/g, '')
    //                     .split(',')
    //                     .map((n) => Number.parseInt(n, radix));
    //                 return { ...acc, parser: undefined };
    //             }
    //             return acc;
    //         }
    //         return parserStart({ ...acc, parser: parserStart }, s);
    //     }
    fn parse_reference_index(acc: &mut ReduceResults, s: &str) -> ReduceResults {
        let mut json = String::new();

        fn parser_start(acc: &mut ReduceResults, s: &str) -> ReduceResults {
            if s == REF_INDEX_BEGIN {
                json.push_str(s);
                return acc.clone();
            }
            if special_character_map().contains(&s.chars().next().unwrap()) {
                return acc.clone();
            }
            // A Reference Index was not found.
            parser_main(acc, s)
        }

        fn parser(acc: &mut ReduceResults, s: &str) -> ReduceResults {
            json.push_str(s);
            if s == REF_INDEX_END {
                ref_index = json
                    .replace(&[' ', '[', ']', '\n'][..], "")
                    .split(',')
                    .map(|n| n.parse::<usize>().unwrap())
                    .collect();
                return acc.clone();
            }
            acc.clone()
        }
        parser_start(acc, s)
    }

    //     reduce(
    //         pipe(
    //             iter,
    //             opConcatMap((a) => [...a]),
    //         ),
    //         parserMain,
    //         {
    //             nodes: [root],
    //             root,
    //             stack: [{ node: root, s: '' }],
    //             parser: parseReferenceIndex,
    //         },
    //     );
    let mut stack = vec![Stack {
        node: Rc::new(RefCell::new(root)),
        s: String::new(),
    }];
    let mut nodes = vec![Rc::new(RefCell::new(root))];
    let mut parser = Some(Box::new(parse_reference_index));
    for value in iter {
        for s in value.chars() {
            if let Some(p) = &parser {
                parser = Some(p(&mut ReduceResults {
                    root: root.clone(),
                    nodes: nodes.clone(),
                    stack: stack.clone(),
                    parser,
                }, &s.to_string()));
            } else {
                parser = Some(Box::new(parser_main(
                    &mut ReduceResults {
                        root: root.clone(),
                        nodes: nodes.clone(),
                        stack: stack.clone(),
                        parser,
                    },
                    &s.to_string(),
                )));
            }
        }
    }
    CspellTrieRoot(root)
}

// function stringToCharSet(values: string): Record<string, boolean | undefined> {
//     const set: Record<string, boolean | undefined> = Object.create(null);
//     const len = values.length;
//     for (let i = 0; i < len; ++i) {
//         set[values[i]] = true;
//     }
//     return set;
// }
fn string_to_char_set(values: &str) -> HashSet<char> {
    let mut set = HashSet::new();
    for c in values.chars() {
        set.insert(c);
    }
    set
}

// function stringToCharMap(values: readonly (readonly [string, string])[]): Record<string, string | undefined> {
//     return createStringLookupMap(values);
// }

fn string_to_char_map(values: &[(String, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in values {
        map.insert(k.clone(), v.clone());
    }
    map
}

// function createStringLookupMap<T>(values: readonly (readonly [string, T])[]): Record<string, T | undefined> {
//     const map: Record<string, T | undefined> = Object.create(null);
//     const len = values.length;
//     for (let i = 0; i < len; ++i) {
//         map[values[i][0]] = values[i][1];
//     }
//     return map;
// }

fn create_string_lookup_map<T>(values: &[(String, T)]) -> HashMap<String, T> {
    let mut map = HashMap::new();
    for (k, v) in values {
        map.insert(k.clone(), v.clone());
    }
    map
}

// /**
//  * Allows an iterable to be shared by multiple consumers.
//  * Each consumer takes from the iterable.
//  * @param iterable - the iterable to share
//  */
// function tapIterable<T>(iterable: Iterable<T>): Iterable<T> {
//     let lastValue: IteratorResult<T>;
//     let iter: Iterator<T> | undefined;
//
//     function getNext(): IteratorResult<T> {
//         if (lastValue && lastValue.done) {
//             return { ...lastValue };
//         }
//         iter = iter || iterable[Symbol.iterator]();
//         lastValue = iter.next();
//         return lastValue;
//     }
//
//     function* iterableFn() {
//         let next: IteratorResult<T>;
//         while (!(next = getNext()).done) {
//             yield next.value;
//         }
//     }
//
//     return {
//         [Symbol.iterator]: iterableFn,
//     };
// }
fn tap_iterable<T>(iterable: impl IntoIterator<Item=T>) -> impl Iterator<Item=T> {
    iterable.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn test_parse_header() {
    //     let input = vec![
    //         "TrieXv4".to_string(),
    //         "base=10".to_string(),
    //         "__DATA__".to_string(),
    //     ];
    //     let (counter, header) = parse_stream(&input).unwrap();
    //     assert_eq!(counter, 3);
    //     assert_eq!(header.version.to_u8(), 4);
    //     assert_eq!(header.base, 10);
    // }

    #[test]
    fn test_parse_body_word_end() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec!["a$".to_string(), "b$".to_string(), "c$".to_string()];
        let trie = parse_stream(10, &input);
        assert!(trie.contains("a"));
        assert!(trie.contains("b"));
        assert!(trie.contains("c"));
        assert!(!trie.contains("d"));
        assert!(!trie.contains("ab"));
        assert!(!trie.contains("abc"));
    }

    #[test]
    fn test_parse_body_escape() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec![
            "a\\$".to_string(),
            "b$".to_string(),
            "c$".to_string(),
            "<2def$".to_string(),
        ];
        let trie = parse_body(&input, &header);
        assert!(!trie.contains("a"));
        assert!(trie.contains("a$b"));
        assert!(trie.contains("a$c"));
        assert!(trie.contains("def"));
    }

    #[test]
    fn test_parse_body_remove() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec!["a$word$<3no$".to_string()];
        let trie = parse_body(&input, &header);
        let mut v = trie.to_vec();
        v.sort();
        assert_eq!(v, vec!["a", "no", "word"]);
    }

    #[test]
    fn test_parse_body_absolute_reference() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 10,
        };
        let input = vec!["apple$<<<n$<banb#1;".to_string()];
        let trie = parse_body(&input, &header);
        let mut v = trie.to_vec();
        v.sort();
        assert_eq!(v, vec!["an", "apple", "banbn", "banbpple"]);
    }

    #[test]
    fn test_parse_body_absolute_reference_2() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 32,
        };
        let input = vec![r"\'cause$5sup$3tis$2wa#9;<4\0th$2$".to_string()];
        let trie = parse_body(&input, &header);
        let mut v = trie.to_vec();
        v.sort();
        assert_eq!(v, vec!["0", "0th", "'cause", "'sup", "'tis", "'twas"]);
    }

    #[test]
    fn test_parse_body_absolute_reference_3() {
        let header = Header {
            version: Version("TrieXv4".to_string()),
            base: 32,
        };
        let input = vec![r"\'cause$5sup$3tis$2wa#9;<4\0th$2$\1st$2$\2nd$2$\3r#g;".to_string()];
        let trie = parse_body(&input, &header);
        let mut v = trie.to_vec();
        v.sort();
        assert_eq!(
            v,
            vec![
                "'cause", "'sup", "'tis", "'twas", "0", "0th", "1", "1st", "2", "2nd", "3rd"
            ]
        );
    }
}