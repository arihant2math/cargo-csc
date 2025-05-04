use std::{
    any,
    fmt::format,
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
};

use anyhow::bail;
use clap::{Args, Parser};

mod trie;

pub use trie::Trie;

struct MultiTrie {
    pub inner: Vec<Trie>,
}

impl MultiTrie {
    fn new() -> Self {
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
        for part in &parts {
            if !self.contains(&part.to_ascii_lowercase()) {
                // check if part is fully numeric
                if part.chars().all(|c| c.is_numeric()) {
                    continue;
                } else {
                    println!("Parts: {:?}", &parts);
                    println!("Word not found: {}", part);
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The path to the folder to search
    search_path: PathBuf,
    /// Which files/folders to exclude from the search
    #[clap(short, long)]
    exclude: Vec<String>,
    #[clap(long)]
    max_depth: Option<usize>,
    #[clap(long, default_value_t = false)]
    follow_symlinks: bool,
    #[clap(long)]
    max_filesize: Option<u64>,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub enum CliArgs {
    /// Check for typos
    Check(CheckArgs),
    Cache,
}

fn handle_node(words: &MultiTrie, node: &tree_sitter::Node, source_code: &str) {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let text = &source_code[start_byte as usize..end_byte as usize];
    if node.is_named() {
        for word in text.split_whitespace() {
            if word.len() > 1 && !words.handle_identifier(word) {
                let line = node.start_position().row + 1;
                let column = node.start_position().column + 1;
                println!("TYPO at {line}:{column}: {}", word);
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        handle_node(words, &child, source_code);
    }
}

fn compile_wordlist(path: &str) -> anyhow::Result<()> {
    let mut trie = Trie::new();
    trie.append_wordlist(format!("wordlists/{path}.txt"))?;
    let data = trie.dump();
    let path = format!("wordlists/{}.bin", path);
    fs::write(path, data)?;
    Ok(())
}

fn get_or_compile_wordlist(name: &str) -> anyhow::Result<Trie> {
    let path = format!("wordlists/{}.bin", name);
    if !PathBuf::from(&path).exists() {
        compile_wordlist(name)?;
    }
    Ok(Trie::load_from_file(path)?)
}

fn get_trie(file: &PathBuf) -> MultiTrie {
    let mut trie = MultiTrie::new();
    let tries = vec!["extra", "software_terms", "words"];
    match get_file_extension(file).unwrap().as_str() {
        "rs" => {
            trie.inner.push(get_or_compile_wordlist("rust").unwrap());
        }
        e => {
            panic!("Unsupported file type: {:?}", e);
        }
    }
    for name in tries {
        let trie_instance = get_or_compile_wordlist(name).unwrap();
        trie.inner.push(trie_instance);
    }
    trie
}

fn get_file_extension(file: &PathBuf) -> Option<String> {
    file.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string())
}

fn get_code(
    path: &PathBuf,
) -> anyhow::Result<(String, tree_sitter::Parser)> {
    let file = File::open(path)?;
    let mut reader = io::BufReader::new(file);
    let mut source_code = String::new();
    reader.read_to_string(&mut source_code)?;
    let mut parser = tree_sitter::Parser::new();
    match get_file_extension(path).unwrap().as_str() {
        "rs" => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        }
        e => {
            bail!("Unsupported file type: {}", e);
        }
    }
    Ok((source_code, parser))
}

fn inner(file: &PathBuf) -> anyhow::Result<()> {
    let (source_code, mut parser) = get_code(file)?;

    let dict = get_trie(file);
    let tree = parser.parse(&source_code, None).unwrap();
    let root_node = tree.root_node();
    handle_node(&dict, &root_node, &source_code);
    Ok(())
}

fn check(args: CheckArgs) -> anyhow::Result<()> {
    let mut builder = ignore::WalkBuilder::new(&args.search_path);
    builder.max_depth(args.max_depth);
    builder.follow_links(args.follow_symlinks);
    builder.max_filesize(args.max_filesize);
    for exclude in &args.exclude {
        builder.add_custom_ignore_filename(exclude);
    }
    let walker = builder.build();
    let mut files = Vec::new();
    for entry in walker {
        match entry {
            Ok(entry) => {
                if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    let path = entry.path();
                    files.push(path.to_path_buf());
                }
            }
            Err(err) => {
                eprintln!("Error: {}", err);
            }
        }
    }

    for file in files {
        inner(&file)?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    match args {
        CliArgs::Check(args) => {
            check(args)?;
        }
        CliArgs::Cache => {
            // list all txt files in wordlists
            let lists = fs::read_dir("wordlists")?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.extension()?.to_str()? == "txt" {
                        Some(path.file_stem()?.to_str()?.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            // compile each wordlist
            for list in lists {
                let mut trie = Trie::new();
                trie.append_wordlist(&list).unwrap();
                let data = trie.dump();
                let path = format!("wordlists/{}.bin", &list);
                fs::write(path, data)?;
            }
        }
    }
    Ok(())
}
