use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::{Context, bail};
use args::{CacheCommand, CheckArgs, CliArgs};
use clap::Parser;
use dashmap::DashMap;
use inquire::Confirm;
use tokio::{sync::Mutex, task, time::Instant};
use url::Url;

mod args;
mod code;
mod cspell;
mod dictionary;
mod filesystem;
pub mod git;
mod multi_trie;
mod settings;
mod trie;

pub use code::{Typo, get_code, handle_node};
pub use dictionary::Dictionary;
pub use filesystem::{cache_path, store_path};
pub use multi_trie::MultiTrie;
pub use settings::Settings;
pub use trie::Trie;

use crate::{
    args::{ContextArgs, OutputFormat, TraceArgs},
    dictionary::{DictCacheStore, dict_cache_store_location},
};
use crate::settings::DictionaryName;

pub type HashSet<T> = ahash::HashSet<T>;
pub type HashMap<K, V> = ahash::HashMap<K, V>;

pub struct CheckContext {
    pub dictionaries: HashMap<String, Trie>,
    pub settings: Settings,
}

struct MergedSettings {
    args: Box<dyn ContextArgs + Send + Sync>,
    settings: Settings,
}

impl MergedSettings {
    fn new(args: Box<dyn ContextArgs + Send + Sync>, settings: Settings) -> Self {
        Self { args, settings }
    }

    fn root_path(&self) -> PathBuf {
        if self.args.dir().is_absolute() {
            self.args.dir()
        } else {
            std::env::current_dir().unwrap()
        }
    }

    fn dictionaries(&self) -> Vec<Dictionary> {
        let mut dictionaries = Vec::with_capacity(
            self.args.extra_dictionaries().len() + self.settings.dictionary_definitions.len(),
        );
        for extra in &self.args.extra_dictionaries() {
            if let Ok(dictionary) = Dictionary::new_with_path(PathBuf::from(extra)) {
                dictionaries.push(dictionary);
            }
        }
        for def in &self.settings.dictionary_definitions {
            dictionaries.push(Dictionary::new_custom(def.clone(), self.root_path()));
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
                    eprintln!("Failed to load dictionary from store: {e}");
                }
            }
        }
        dictionaries
    }

    fn base_dictionaries(&self) -> Vec<String> {
        let mut dictionaries = self
            .settings
            .dictionaries
            .iter()
            .map(DictionaryName::name)
            .collect::<Vec<_>>();
        dictionaries.extend(self.args.extra_dictionaries());
        dictionaries
    }

    fn verbose(&self) -> bool {
        self.args.verbose()
    }

    fn jobs(&self) -> usize {
        self.args.jobs().unwrap_or_else(num_cpus::get)
    }
}

struct SharedRuntimeContext {
    // None means the dictionary is not loaded
    dictionaries: DashMap<String, Arc<Trie>>,
    settings: MergedSettings,
}

impl SharedRuntimeContext {
    fn new(settings: MergedSettings) -> Self {
        let dictionaries = DashMap::new();
        Self {
            dictionaries,
            settings,
        }
    }

    fn custom_trie(&self) -> anyhow::Result<Trie> {
        let v = Dictionary::new_from_strings(&self.settings.settings.words);
        v.compile()
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

fn get_multi_trie<P: AsRef<Path>>(
    path: Option<P>,
    context: Arc<SharedRuntimeContext>,
) -> anyhow::Result<MultiTrie> {
    if let Some(ref path) = path {
        if path.as_ref().is_dir() {
            bail!("Path is a directory: {}", path.as_ref().display());
        }
    }
    let mut trie = MultiTrie::new();
    let tries = context.get_base_dictionaries();

    for name in tries {
        let trie_instance = context
            .dictionaries
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("Dictionary not found: {}", name))?
            .clone();
        trie.inner.push(trie_instance);
    }
    trie.inner.push(Arc::new(context.custom_trie()?));
    Ok(trie)
}

#[tokio::main]
async fn handle_file(
    context: Arc<SharedRuntimeContext>,
    file_receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<PathBuf>>>,
    result_sender: tokio::sync::mpsc::Sender<CheckFileResult>,
) -> anyhow::Result<()> {
    if context.settings.verbose() {
        println!("Starting thread #{:?}", thread::current().id());
    }
    loop {
        let file = if let Some(f) = file_receiver.lock().await.recv().await {
            f
        } else {
            break;
        };
        let (source_code, mut parser) = get_code(&file)
            .await
            .context(format!("Failed to get code for file: {}", file.display()))?;

        let dict = get_multi_trie(Some(&file), context.clone()).context(format!(
            "Failed to load dictionary set for file: {}",
            file.display()
        ))?;
        let tree = parser.parse(&source_code, None).unwrap();
        let root_node = Box::new(tree.root_node());
        let typos = handle_node(&dict, &root_node, &source_code.into());
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

fn load_dictionaries(context: Arc<SharedRuntimeContext>) -> anyhow::Result<()> {
    let c = context.get_dictionaries();
    let base_dictionaries = context.get_base_dictionaries();
    for dict in c {
        let names = dict.get_names()?;
        if !base_dictionaries.iter().any(|x| names.contains(x)) {
            // Don't load pointless tries
            continue;
        }
        let trie = Arc::new(dict.compile()?);
        for name in names {
            // TODO: handle overwrites
            context.dictionaries.insert(name.clone(), trie.clone());
        }
    }
    Ok(())
}

async fn check(args: CheckArgs) -> anyhow::Result<()> {
    let settings = Settings::load(args.settings.clone().map(|p| p.display().to_string()));
    // Generate context
    let context = Arc::new(SharedRuntimeContext::new(MergedSettings::new(
        Box::new(args.clone()),
        settings,
    )));
    let load_dictionaries_context = context.clone();
    let dictionary_loader = task::spawn_blocking(|| load_dictionaries(load_dictionaries_context));
    let (file_sender, file_receiver) = tokio::sync::mpsc::channel(256);
    let file_loader = task::spawn({
        let context = context.clone();
        let glob = args.glob.clone();
        async move {
            // Find files, also send them to file_sender
            let pattern =
                glob::Pattern::new(glob.as_ref().unwrap_or(&"**/*.*".to_string())).unwrap();
            let walker = ignore::WalkBuilder::new(context.settings.args.dir()).build();
            let mut files = vec![];
            for file in walker.flatten() {
                if file.path().is_file() && pattern.matches_path(file.path()) {
                    file_sender.send(file.path().to_path_buf()).await.unwrap();
                    files.push(file.path().to_path_buf());
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
        println!("Found {total_files} files");
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
    let output = context.settings.args.output().unwrap_or(OutputFormat::Text);
    if matches!(&output, OutputFormat::Json) {
        todo!();
    }
    while let Some(result) = result_receiver.recv().await {
        counter += 1;
        if context.settings.verbose() || args.progress {
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
            let diagnostic: miette::Report = typo
                .to_diagnostic(&result.file.display().to_string())
                .into();
            println!("{diagnostic:?}");
        }
    }

    if context.settings.verbose() {
        println!("All files processed");
    }
    let start = Instant::now();
    let mut printed = false;
    loop {
        let now = Instant::now();
        if !printed && now - start > Duration::from_secs(1) {
            println!("Waiting for threads to finish...");
            printed = true;
        }
        if now - start > Duration::from_secs(5) {
            println!("Threads are taking too long to finish, exiting...");
            std::process::exit(1);
        }
        if threads.iter().all(thread::JoinHandle::is_finished) {
            break;
        }
    }
    for thread in threads {
        thread.join().unwrap()?;
    }
    Ok(())
}

fn trace(args: &TraceArgs) -> anyhow::Result<()> {
    let settings = Settings::load(args.settings.clone().map(|p| p.display().to_string()));
    // Generate context
    let context = Arc::new(SharedRuntimeContext::new(MergedSettings::new(
        Box::new(args.clone()),
        settings,
    )));
    let load_dictionaries_context = context.clone();
    load_dictionaries(load_dictionaries_context)?;
    let mut found = false;
    for kv in &context.dictionaries {
        let name = kv.key();
        let dict = kv.value();
        if dict.contains(&args.word) {
            println!("Found \'{}\' in dictionary {}", args.word, name);
            found = true;
        }
    }
    if !found {
        println!("Did not find \'{}\' in any dictionary", args.word);
    }
    Ok(())
}

async fn cache(args: CacheCommand) -> anyhow::Result<()> {
    match args {
        CacheCommand::Build => {
            let dict_dir = store_path();
            // List all files in the directory
            let mut files = vec![];
            for entry in fs::read_dir(dict_dir)? {
                let entry = entry?;
                let path = entry.path();
                files.push(path);
            }
            for path in files {
                let _ = Dictionary::new_with_path(path)?.compile()?;
            }
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
        CacheCommand::List => {
            let cache_info = DictCacheStore::load_from_file(dict_cache_store_location()?)?;
            for k in cache_info.0.keys() {
                println!("- {k}");
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
        CliArgs::Trace(ref args) => {
            trace(args)?;
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
                        let content = response.bytes().await?.to_vec();
                        let end = url
                            .path_segments()
                            .and_then(|mut s| s.next_back())
                            .unwrap_or_default();
                        if Path::new(end)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                        {
                            let zip_path = store_path().join(end);
                            if zip_path.exists() {
                                if !args.yes {
                                    let confirm = Confirm::new("File already exists, overwrite?")
                                        .with_default(false)
                                        .prompt()?;
                                    if !confirm {
                                        println!("Aborting");
                                        return Ok(());
                                    }
                                }
                                if zip_path.is_dir() {
                                    fs::remove_dir_all(&zip_path).context(format!(
                                        "Failed to remove existing dir: {}",
                                        zip_path.display()
                                    ))?;
                                } else {
                                    fs::remove_file(&zip_path).context(format!(
                                        "Failed to remove existing file: {}",
                                        zip_path.display()
                                    ))?;
                                }
                            }
                            let mut file = fs::File::create(&zip_path)?;
                            file.write_all(&content)?;
                            let mut archive = zip::ZipArchive::new(fs::File::open(zip_path)?)?;
                            let base_out_path = store_path().join(
                                url.path_segments()
                                    .unwrap()
                                    .next_back()
                                    .unwrap()
                                    .strip_suffix(".zip")
                                    .unwrap(),
                            );
                            for i in 0..archive.len() {
                                let mut file = archive.by_index(i)?;
                                let outpath = base_out_path.join(file.name());
                                if file.is_dir() {
                                    fs::create_dir_all(&outpath)?;
                                } else {
                                    let mut outfile = fs::File::create(&outpath)?;
                                    std::io::copy(&mut file, &mut outfile)?;
                                }
                            }
                        } else {
                            let path = store_path()
                                .join(url.path_segments().unwrap().next_back().unwrap());
                            if path == store_path() {
                                bail!("Cannot install to cache directory");
                            }
                            if path.exists() {
                                if !args.yes {
                                    let confirm = Confirm::new(&format!(
                                        "File {path} already exists, overwrite?",
                                        path = path.display()
                                    ))
                                    .with_default(false)
                                    .prompt()?;
                                    if !confirm {
                                        println!("Aborting");
                                        return Ok(());
                                    }
                                }
                                if path.is_dir() {
                                    fs::remove_dir_all(&path).context(format!(
                                        "Failed to remove existing dir: {}",
                                        path.display()
                                    ))?;
                                } else {
                                    fs::remove_file(&path).context(format!(
                                        "Failed to remove existing file: {}",
                                        path.display()
                                    ))?;
                                }
                            }
                            let mut file = fs::File::create(path)?;
                            file.write_all(&content)?;
                        }
                    } else {
                        bail!(
                            "Failed to download file from {}: {}",
                            url,
                            response.status()
                        );
                    }
                }
            }
        }
        CliArgs::ImportCspell => {
            cspell::import()?;
        }
    }
    Ok(())
}
