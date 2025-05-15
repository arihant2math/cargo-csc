use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    /// JSON output
    Json,
    /// Text output
    Text,
}

pub trait ContextArgs {
    fn dir(&self) -> PathBuf;
    fn verbose(&self) -> bool;
    fn extra_dictionaries(&self) -> Vec<String>;
    fn exclude(&self) -> Vec<String>;
    fn max_depth(&self) -> Option<usize>;
    fn follow_symlinks(&self) -> bool;
    fn max_filesize(&self) -> Option<u64>;
    fn jobs(&self) -> Option<usize>;
    fn settings(&self) -> Option<PathBuf>;
    fn output(&self) -> Option<OutputFormat>;
}

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The path to the folder to search
    pub dir: PathBuf,
    pub glob: Option<String>,
    /// Verbose output
    #[clap(short, long, default_value_t = false)]
    pub verbose: bool,
    #[clap(short, long, default_value_t = false)]
    pub progress: bool,
    /// Which files/folders to exclude from the search
    #[clap(long)]
    pub exclude: Vec<String>,
    #[clap(long)]
    pub extra_dictionaries: Vec<String>,
    #[clap(long)]
    pub max_depth: Option<usize>,
    #[clap(long, default_value_t = false)]
    pub follow_symlinks: bool,
    #[clap(long)]
    pub max_filesize: Option<u64>,
    #[clap(short, long)]
    pub jobs: Option<usize>,
    #[clap(long)]
    pub settings: Option<PathBuf>,
    #[clap(long)]
    pub output: Option<OutputFormat>,
}

impl ContextArgs for CheckArgs {
    fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn verbose(&self) -> bool {
        self.verbose
    }

    fn extra_dictionaries(&self) -> Vec<String> {
        self.extra_dictionaries.clone()
    }

    fn exclude(&self) -> Vec<String> {
        self.exclude.clone()
    }

    fn max_depth(&self) -> Option<usize> {
        self.max_depth
    }

    fn follow_symlinks(&self) -> bool {
        self.follow_symlinks
    }

    fn max_filesize(&self) -> Option<u64> {
        self.max_filesize
    }

    fn jobs(&self) -> Option<usize> {
        self.jobs
    }

    fn settings(&self) -> Option<PathBuf> {
        self.settings.clone()
    }

    fn output(&self) -> Option<OutputFormat> {
        self.output.clone()
    }
}

#[derive(Clone, Debug, Args)]
pub struct TraceArgs {
    pub word: String,
    /// The path to the folder to search
    pub dir: PathBuf,
    pub glob: Option<String>,
    /// Verbose output
    #[clap(short, long, default_value_t = false)]
    pub verbose: bool,
    #[clap(long)]
    pub settings: Option<PathBuf>,
    #[clap(long)]
    pub output: Option<OutputFormat>,
}

impl ContextArgs for TraceArgs {
    fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn verbose(&self) -> bool {
        self.verbose
    }

    fn extra_dictionaries(&self) -> Vec<String> {
        vec![]
    }

    fn exclude(&self) -> Vec<String> {
        vec![]
    }

    fn max_depth(&self) -> Option<usize> {
        None
    }

    fn follow_symlinks(&self) -> bool {
        true
    }

    fn max_filesize(&self) -> Option<u64> {
        None
    }

    fn jobs(&self) -> Option<usize> {
        None
    }

    fn settings(&self) -> Option<PathBuf> {
        self.settings.clone()
    }

    fn output(&self) -> Option<OutputFormat> {
        self.output.clone()
    }
}

#[derive(Clone, Debug, Args)]
pub struct InstallArgs {
    pub uri: String,
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
    Trace(TraceArgs),
    Install(InstallArgs),
    /// Import cspell dictionaries
    ImportCspell,
}
