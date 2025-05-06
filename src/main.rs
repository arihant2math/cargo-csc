use std::io::Write;
use std::thread;
use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use url::Url;

mod code;
mod dictionary;
mod filesystem;
mod multi_trie;
mod settings;
mod trie;

pub use code::{Typo, get_code, handle_node};
pub use dictionary::{Dictionary, Rule};
pub use filesystem::{cache_path, get_file_extension, store_path};
pub use multi_trie::MultiTrie;
pub use settings::Settings;
pub use trie::Trie;

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The path to the folder to search
    dir: PathBuf,
    glob: Option<String>,
    /// Verbose output
    #[clap(short, long, default_value_t = false)]
    verbose: bool,
    #[clap(short, long, default_value_t = false)]
    progress: bool,
    /// Which files/folders to exclude from the search
    #[clap(long)]
    exclude: Vec<String>,
    #[clap(long)]
    extra_dictionaries: Vec<String>,
    #[clap(long)]
    max_depth: Option<usize>,
    #[clap(long, default_value_t = false)]
    follow_symlinks: bool,
    #[clap(long)]
    max_filesize: Option<u64>,
    #[clap(short, long)]
    jobs: Option<usize>,
    #[clap(long)]
    settings: Option<PathBuf>,
}

#[derive(Clone, Debug, Args)]
pub struct InstallArgs {
    uri: String,
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
    Install(InstallArgs),
}

pub struct CheckContext {
    pub dictionaries: HashMap<String, Trie>,
    pub settings: Settings,
}

struct MergedSettings {
    args: CheckArgs,
    settings: Settings,
}

impl MergedSettings {
    fn new(args: CheckArgs, settings: Settings) -> Self {
        Self { args, settings }
    }

    fn dictionaries(&self) -> Vec<Dictionary> {
        let mut dictionaries = Vec::with_capacity(
            self.args.extra_dictionaries.len() + self.settings.dictionary_definitions.len(),
        );
        for extra in &self.args.extra_dictionaries {
            if let Ok(dictionary) = Dictionary::new_with_path(PathBuf::from(extra)) {
                dictionaries.push(dictionary);
            }
        }
        for def in self.settings.dictionary_definitions.iter() {
            if let Ok(dictionary) = Dictionary::new_with_path(PathBuf::from(&def.path)) {
                dictionaries.push(dictionary);
            }
        }
        // check store_path for dictionaries
        for entry in fs::read_dir(store_path()).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext.to_str().unwrap() == "bin" {
                    continue;
                }
            }
            match Dictionary::new_with_path(path) {
                Ok(dictionary) => dictionaries.push(dictionary),
                Err(e) => {
                    eprintln!("Failed to load dictionary from store: {}", e);
                }
            }
        }
        dictionaries
    }

    fn base_dictionaries(&self) -> Vec<String> {
        let mut dictionaries = self.settings.dictionaries.clone();
        dictionaries.extend(self.args.extra_dictionaries.clone());
        dictionaries
    }

    fn verbose(&self) -> bool {
        self.args.verbose
    }

    fn jobs(&self) -> usize {
        self.args.jobs.unwrap_or_else(|| num_cpus::get())
    }
}

struct SharedCheckContext {
    // None means the dictionary is not loaded
    dictionaries: DashMap<String, Trie>,
    settings: MergedSettings,
}

impl SharedCheckContext {
    fn new(settings: MergedSettings) -> Self {
        let dictionaries = DashMap::new();
        Self {
            dictionaries,
            settings,
        }
    }

    fn custom_trie(&self) -> anyhow::Result<Trie> {
        let v = Dictionary::new_from_strings(self.settings.settings.words.clone());
        Ok(v.compile()?)
    }

    fn get_base_dictionaries(&self) -> Vec<String> {
        self.settings.base_dictionaries()
    }

    fn get_dictionaries(&self) -> Vec<Dictionary> {
        self.settings.dictionaries()
    }
}

struct CheckFileResult {
    file: PathBuf,
    typos: Vec<Typo>,
}

fn get_multi_trie(path: &PathBuf, context: Arc<SharedCheckContext>) -> anyhow::Result<MultiTrie> {
    if path.is_dir() {
        bail!("Path is a directory: {}", path.display());
    }
    let mut trie = MultiTrie::new();
    let mut tries = context.get_base_dictionaries().clone();
    match get_file_extension(path).unwrap().as_str() {
        "c" => {
            tries.push("c".to_string());
        }
        "cpp" => {
            tries.push("cpp".to_string());
        }
        "rs" => {
            tries.push("rust".to_string());
        }
        "py" => {
            tries.push("python".to_string());
        }
        e => {
            eprintln!("Unsupported file type: {}", e);
        }
    }
    for name in tries {
        let trie_instance = context
            .dictionaries
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("Dictionary not found: {}", name))?
            .clone();
        trie.inner.push(trie_instance);
    }
    trie.inner.push(context.custom_trie()?);
    Ok(trie)
}

#[tokio::main]
async fn handle_file(
    context: Arc<SharedCheckContext>,
    file_receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<PathBuf>>>,
    result_sender: tokio::sync::mpsc::Sender<CheckFileResult>,
) -> anyhow::Result<()> {
    if context.settings.verbose() {
        println!("Starting thread #{:?}" , thread::current().id());
    }
    loop {
        let file = match file_receiver.lock().await.recv().await {
            Some(f) => f,
            None => {
                break;
            }
        };
        let (source_code, mut parser) = get_code(&file)
            .await
            .context(format!("Failed to get code for file: {}", file.display()))?;

        let dict = get_multi_trie(&file, context.clone()).context(format!(
            "Failed to load dictionary set for file: {}",
            file.display()
        ))?;
        let tree = parser.parse(&source_code, None).unwrap();
        let root_node = Box::new(tree.root_node());
        let typos = handle_node(&dict, &root_node, &source_code);
        let result = CheckFileResult {
            file: file.clone(),
            typos,
        };
        result_sender.send(result).await.context(format!(
            "Failed to send result for file: {}",
            file.display()
        ))?;
    }
    if context.settings.verbose() {
        println!("Finalizing thread #{:?}", thread::current().id());
    }
    Ok(())
}

async fn check(args: CheckArgs) -> anyhow::Result<()> {
    let settings = settings::Settings::load(args.settings.clone().map(|p| p.display().to_string()));
    // Generate context
    let context = Arc::new(SharedCheckContext::new(MergedSettings::new(args, settings)));
    let dictionary_loader = task::spawn_blocking({
        let context = context.clone();
        move || {
            let c = context.get_dictionaries();
            for dict in c {
                let trie = dict.compile()?;
                context.dictionaries.insert(trie.options.name.clone(), trie);
            }
            Ok::<(), anyhow::Error>(())
        }
    });
    let (file_sender, file_receiver) = tokio::sync::mpsc::channel(256);
    let file_loader = task::spawn({
        let context = context.clone();
        async move {
            // Find files, also send them to file_sender
            // TODO: actually do it
            let pattern = glob::Pattern::new(context.settings.args.glob.as_ref().unwrap_or(&"**/*.*".to_string())).unwrap();
            let walker = ignore::WalkBuilder::new(context.settings.args.dir.clone()).build();
            let mut files = vec![];
            for file in walker {
                if let Ok(file) = file {
                    if file.path().is_file() && pattern.matches_path(file.path()) {
                        file_sender.send(file.path().to_path_buf()).await.unwrap();
                        files.push(file.path().to_path_buf());
                    }
                }
            }
            files
        }
    });

    let (res, files) = tokio::join!(dictionary_loader, file_loader);
    res??;
    let files = files?;
    if files.is_empty() {
        eprintln!("No files found");
        return Ok(());
    }
    let total_files = files.len();
    if total_files == 1 {
        println!("Found 1 file");
    } else {
        println!("Found {} files", total_files);
    }

    let (result_sender, mut result_receiver) = tokio::sync::mpsc::channel(256);
    let file_receiver = Arc::new(Mutex::new(file_receiver));
    let num_threads = context.settings.jobs();
    let threads = (0..num_threads)
        .map(|_| {
            let context = context.clone();
            let file_receiver = file_receiver.clone();
            let result_sender = result_sender.clone();
            thread::spawn(move || handle_file(context, file_receiver, result_sender))
        })
        .collect::<Vec<_>>();
    let mut counter = 0;
    drop(result_sender);
    while let Some(result) = result_receiver.recv().await {
        counter += 1;
        if context.settings.verbose() || context.settings.args.progress {
            if result.typos.is_empty() {
                println!(
                    "[{counter}/{total_files}] {file}: No typos found",
                    file = result.file.display()
                );
            } else if result.typos.len() == 1 {
                println!(
                    "[{counter}/{total_files}] {file}: Found 1 typo",
                    file = result.file.display()
                );
            } else {
                println!(
                    "[{counter}/{total_files}] {file}: Found {} typos",
                    result.typos.len(),
                    file = result.file.display()
                );
            }
        }
        for typo in &result.typos {
            eprintln!(
                "{file}:{line}:{column}: Unknown word {word}",
                file = result.file.display(),
                line = typo.line,
                column = typo.column,
                word = typo.word
            );
        }
    }
    println!("Waiting for threads to finish ...");
    for thread in threads {
        thread.join().unwrap()?;
    }
    Ok(())
}

async fn cache(args: CacheCommand) -> anyhow::Result<()> {
    match args {
        CacheCommand::Build => {
            todo!();
        }
        CacheCommand::Clear => {
            let cache_dir = cache_path();
            if cache_dir.exists() {
                fs::remove_dir_all(&cache_dir).context(format!(
                    "Failed to remove cache directory: {}",
                    cache_dir.display()
                ))?;
            } else {
                eprintln!("Cache directory does not exist: {}", cache_dir.display());
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    match args {
        CliArgs::Check(args) => {
            check(args).await?;
        }
        CliArgs::Cache(args) => {
            cache(args).await?;
        }
        CliArgs::Install(args) => {
            // Try path
            enum InstallType {
                Path(PathBuf),
                Url(Url),
            }
            let path = PathBuf::from(&args.uri);
            let install_type = if path.exists() {
                InstallType::Path(path)
            } else {
                InstallType::Url(Url::parse(&args.uri)?)
            };
            match install_type {
                InstallType::Path(ref path) => {
                    fs::copy(path, store_path().join(path.file_name().unwrap()))?;
                }
                InstallType::Url(ref url) => {
                    let response = reqwest::get(url.clone()).await?;
                    if response.status().is_success() {
                        let mut file = fs::File::create(
                            store_path().join(url.path_segments().unwrap().last().unwrap()),
                        )?;
                        let mut content = response.bytes().await?.to_vec();
                        file.write_all(&mut content)?;
                    } else {
                        bail!("Failed to download file from URL: {}", url);
                    }
                }
            }
        }
    }
    Ok(())
}
