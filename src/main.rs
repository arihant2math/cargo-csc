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

fn compile_wordlist<P: AsRef<Path>>(path: P, output: P) -> anyhow::Result<()> {
    let trie = Trie::from_wordlist(&path)?;
    let data = trie.dump();
    fs::write(&output, data)?;
    let hash_store_path = store_path().join("wordlist_hashes.json");
    let mut hash_store =
        TrieHashStore::load_from_file(&hash_store_path).unwrap_or_else(|_| TrieHashStore::new());
    let hash = hash_file(path.as_ref())?;

    hash_store
        .0
        .insert(path.as_ref().display().to_string(), hash);
    hash_store.dump_to_file(&hash_store_path)?;
    Ok(())
}

fn hash_file<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let text = fs::read(&path)
        .with_context(|| format!("Failed to read file: {}", path.as_ref().display()))?;
    Ok(blake3::hash(&text).to_hex().to_string())
}

fn get_or_compile_wordlist(
    name: &str,
    definitions: &[settings::DictionaryDefinition],
) -> anyhow::Result<Trie> {
    let definition = definitions
        .iter()
        .find(|def| def.name == name)
        .cloned()
        .unwrap_or(settings::DictionaryDefinition {
            name: name.to_string(),
            path: store_path()
                .join(format!("{}.txt", name))
                .to_string_lossy()
                .to_string(),
            globs: vec![],
            compile: true,
        });
    if definition.compile {
        let parent = Path::new(&definition.path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let text_path = PathBuf::from(&definition.path);
        let bin_path = parent.join(format!("{}.bin", name));

        let hash_store = TrieHashStore::load_from_file(store_path().join("wordlist_hashes.json"))
            .unwrap_or_else(|_| TrieHashStore::new());
        let hash = hash_file(store_path().join(format!("{}.txt", name)))
            .context(format!("Failed to hash wordlist: {}", name))?;
        if !bin_path.exists() {
            compile_wordlist(&text_path, &bin_path)
                .context(format!("Failed to compile wordlist to trie: {}", name))?;
        }
        if let Some(stored_hash) = hash_store.0.get(name) {
            if stored_hash != &hash {
                compile_wordlist(&text_path, &bin_path)
                    .context(format!("Failed to compile wordlist to trie: {}", name))?;
            }
        } else {
            compile_wordlist(&text_path, &bin_path)
                .context(format!("Failed to compile wordlist to trie: {}", name))?;
        }
        Ok(Trie::load_from_file(bin_path)
            .context(format!("Failed to load trie binary: {}", name))?)
    } else {
        Ok(Trie::from_wordlist(&definition.path)
            .context(format!("Failed to load wordlist: {}", name))?)
    }
}

fn get_trie(file: &PathBuf, settings: &Settings) -> anyhow::Result<MultiTrie> {
    let mut trie = MultiTrie::new();
    let mut tries = settings.dictionaries.clone();
    match get_file_extension(file).unwrap().as_str() {
        "rs" => {
            tries.push("rust".to_string());
        }
        e => {
            panic!("Unsupported file type: {:?}", e);
        }
    }
    for name in tries {
        let trie_instance = get_or_compile_wordlist(&name, &settings.dictionary_definitions)
            .context(format!("Failed to load wordlist: {}", &name))?;
        trie.inner.push(trie_instance);
    }
    let custom_trie = Trie::from_iterator(settings.words.iter().map(|s| s.as_str()));
    trie.inner.push(custom_trie);
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
            let result = check_file(file, settings)
                .context(format!("Failed to check file: {}", file.display()))?;
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
                let text_path = store_path().join(format!("{}.txt", list));
                let bin_path = store_path().join(format!("{}.bin", list));
                compile_wordlist(text_path, bin_path)
                    .context(format!("Failed to compile wordlist: {}", list))?;
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
