use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use rayon::prelude::*;

mod multi_trie;
mod settings;
mod trie;

pub use multi_trie::MultiTrie;
use settings::Settings;
pub use trie::Trie;
use trie::TrieHashStore;

fn store_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.pop();
    path.push("wordlists");
    path
}

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The path to the folder to search
    glob: String,
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

#[derive(Clone, Debug, Subcommand)]
pub enum CacheCommand {
    /// Compile the wordlists
    Build,
    /// Clear the cache
    Clear,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub enum CliArgs {
    /// Check for typos
    Check(CheckArgs),
    #[command(subcommand)]
    Cache(CacheCommand),
}

fn handle_node(words: &MultiTrie, node: &tree_sitter::Node, source_code: &str) -> Vec<Typo> {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let text = &source_code[start_byte as usize..end_byte as usize];
    let mut typos = Vec::new();
    if node.is_named() {
        for word in text.split_whitespace() {
            if word.len() > 1 {
                if let Some(typo) = words.handle_identifier(word) {
                    let line = node.start_position().row + 1;
                    let column = node.start_position().column + 1;
                    let typo = Typo {
                        line,
                        column,
                        word: typo,
                    };
                    typos.push(typo);
                }
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        typos.append(&mut handle_node(words, &child, source_code));
    }
    typos
}

fn compile_wordlist(path: &str) -> anyhow::Result<()> {
    let mut trie = Trie::append_wordlist(store_path().join(format!("{path}.txt")))?;
    let data = trie.dump();
    let out_path = store_path().join(format!("{}.bin", path));
    fs::write(&out_path, data)?;
    let hash_store_path = store_path().join("wordlist_hashes.json");
    let mut hash_store =
        TrieHashStore::load_from_file(&hash_store_path).unwrap_or_else(|_| TrieHashStore::new());
    let hash = hash_file(store_path().join(format!("{path}.txt")))?;

    hash_store.0.insert(path.to_string(), hash);
    hash_store.dump_to_file(&hash_store_path)?;
    Ok(())
}

fn hash_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<String> {
    let text =
        fs::read(&path).with_context(|| format!("Failed to read file: {}", path.as_ref().display()))?;
    Ok(blake3::hash(&text).to_hex().to_string())
}

fn get_or_compile_wordlist(name: &str) -> anyhow::Result<Trie> {
    let path = store_path().join(format!("{}.bin", name));
    let hash_store = TrieHashStore::load_from_file(store_path().join("wordlist_hashes.json"))
        .unwrap_or_else(|_| TrieHashStore::new());
    let hash = hash_file(store_path().join(format!("{}.txt", name)))
        .context(format!("Failed to hash wordlist: {}", name))?;
    if !PathBuf::from(&path).exists() {
        compile_wordlist(name).context(format!("Failed to compile wordlist to trie: {}", name))?;
    }
    if let Some(stored_hash) = hash_store.0.get(name) {
        if stored_hash != &hash {
            compile_wordlist(name)
                .context(format!("Failed to compile wordlist to trie: {}", name))?;
        }
    } else {
        compile_wordlist(name).context(format!("Failed to compile wordlist to trie: {}", name))?;
    }
    Ok(Trie::load_from_file(path).context(format!("Failed to load trie binary: {}", name))?)
}

fn get_trie(file: &PathBuf, settings: &Settings) -> anyhow::Result<MultiTrie> {
    let mut trie = MultiTrie::new();
    let mut tries = vec![
        "custom",
        "extra",
        "software_terms",
        "software_tools",
        "words",
    ];
    match get_file_extension(file).unwrap().as_str() {
        "rs" => {
            tries.push("rust");
        }
        e => {
            panic!("Unsupported file type: {:?}", e);
        }
    }
    for name in tries {
        let trie_instance =
            get_or_compile_wordlist(name).context(format!("Failed to load wordlist: {}", name))?;
        trie.inner.push(trie_instance);
    }
    Ok(trie)
}

fn get_file_extension(file: &PathBuf) -> Option<String> {
    file.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string())
}

fn get_code(path: &PathBuf) -> anyhow::Result<(String, tree_sitter::Parser)> {
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

#[derive(Debug)]
struct Typo {
    line: usize,
    column: usize,
    word: String,
}

struct CheckFileResult {
    file: PathBuf,
    typos: Vec<Typo>,
}

fn check_file(file: &PathBuf, settings: &Settings) -> anyhow::Result<CheckFileResult> {
    let (source_code, mut parser) =
        get_code(file).context(format!("Failed to get code for file: {}", file.display()))?;

    let dict = get_trie(file, settings).context(format!(
        "Failed to load dictionary set for file: {}",
        file.display()
    ))?;
    let tree = parser.parse(&source_code, None).unwrap();
    let root_node = tree.root_node();
    let typos = handle_node(&dict, &root_node, &source_code);
    let result = CheckFileResult {
        file: file.clone(),
        typos,
    };
    Ok(result)
}

fn check(args: CheckArgs, settings: &Settings) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for entry in glob::glob(&args.glob)? {
        match entry {
            Ok(entry) => {
                for exclude in &args.exclude {
                    if glob::Pattern::new(exclude)?.matches_path(&entry) {
                        continue;
                    }
                }
                files.push(entry);
            }
            Err(err) => {
                eprintln!("Globbing Error: {}", err);
            }
        }
    }

    if files.is_empty() {
        bail!("No files found");
    }
    if files.len() > 1 {
        println!("Found {} files", files.len());
    } else {
        println!("Found 1 file");
    }

    files
        .par_iter()
        .try_for_each(|file| -> anyhow::Result<()> {
            let result =
                check_file(file, settings).context(format!("Failed to check file: {}", file.display()))?;
            if !result.typos.is_empty() {
                for typo in result.typos.iter() {
                    println!(
                        "{}:{}:{}: Unknown word: {}",
                        result.file.display(),
                        typo.line,
                        typo.column,
                        typo.word
                    );
                }
            }
            Ok(())
        })?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    let settings = settings::Settings::load(None);

    match args {
        CliArgs::Check(args) => {
            check(args, &settings)?;
        }
        CliArgs::Cache(CacheCommand::Build) => {
            // list all txt files in wordlists
            let lists = fs::read_dir(store_path())?
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
                compile_wordlist(&list)?;
            }
        }
        CliArgs::Cache(CacheCommand::Clear) => {
            // delete all bin files in wordlists
            let lists = fs::read_dir(store_path())?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.extension()?.to_str()? == "bin" {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for list in lists {
                fs::remove_file(list)?;
            }
        }
    }
    Ok(())
}
