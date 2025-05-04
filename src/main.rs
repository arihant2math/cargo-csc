use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use rayon::prelude::*;

mod dictionary;
mod filesystem;
mod multi_trie;
mod settings;
mod trie;

pub use multi_trie::MultiTrie;
use settings::Settings;
pub use trie::Trie;
use trie::TrieHashStore;

fn store_path() -> PathBuf {
    let mut path = std::env::home_dir().expect("Failed to get home directory");
    path.push(".code-spellcheck");
    path.push("wordlists");
    if !path.exists() {
        fs::create_dir_all(&path).expect("Failed to create wordlists directory");
    }
    path
}

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The path to the folder to search
    dir: PathBuf,
    glob: Option<String>,
    /// Which files/folders to exclude from the search
    #[clap(short, long)]
    exclude: Vec<String>,
    #[clap(long)]
    max_depth: Option<usize>,
    #[clap(long, default_value_t = false)]
    follow_symlinks: bool,
    #[clap(long)]
    max_filesize: Option<u64>,
    #[clap(long)]
    settings: Option<PathBuf>,
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

pub fn get_file_extension(file: &PathBuf) -> Option<String> {
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
        "go" => {
            parser.set_language(&tree_sitter_go::LANGUAGE.into())?;
        }
        "rs" => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        }
        "toml" => {
            parser.set_language(&tree_sitter_toml_ng::LANGUAGE.into())?;
        }
        "js" => {
            parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
        }
        "py" => {
            parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
        }
        e => {
            bail!("Unsupported file type: {}", e);
        }
    }
    Ok((source_code, parser))
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

    let dict = filesystem::get_trie(file, settings).context(format!(
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

fn check(args: CheckArgs) -> anyhow::Result<()> {
    let settings = settings::Settings::load(args.settings.map(|p| p.display().to_string()));

    // TODO: path my not be "."
    let mut walker = ignore::WalkBuilder::new(&args.dir);
    for exclude in &args.exclude {
        walker.add_custom_ignore_filename(exclude);
    }

    let pattern = glob::Pattern::new(&args.glob.unwrap_or("**/*.*".to_string()))?;
    let files = walker.build().collect::<Result<Vec<_>, _>>()?;
    let files: Vec<_> = files
        .into_iter()
        .map(|d| d.into_path())
        .filter(|f| f.is_file())
        .filter(|f| pattern.matches_path(f))
        .collect();

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
            let result = check_file(file, &settings)
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

    match args {
        CliArgs::Check(args) => {
            check(args)?;
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
                filesystem::compile_wordlist(text_path, bin_path)
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
