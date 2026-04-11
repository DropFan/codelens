//! CLI argument definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// High performance code statistics tool.
#[derive(Parser, Debug)]
#[command(
    name = "codelens",
    author,
    version,
    about = "High performance code statistics tool",
    after_help = EXAMPLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Directories to analyze (defaults to current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    #[command(flatten)]
    pub filter: FilterArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub advanced: AdvancedArgs,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Analyze code health score.
    Health(HealthArgs),
    /// Detect change hotspots (churn x complexity).
    Hotspot(HotspotArgs),
    /// Track codebase trends with snapshots.
    Trend(TrendArgs),
}

#[derive(Args, Debug)]
pub struct HealthArgs {
    /// Directories to analyze (defaults to current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Number of worst files/directories to show.
    #[arg(long = "worst", default_value = "10")]
    pub worst_n: usize,

    #[command(flatten)]
    pub filter: FilterArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Debug)]
pub struct HotspotArgs {
    /// Directories to analyze (defaults to current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Time window (e.g. 30d, 4w, 6m, 1y, or YYYY-MM-DD).
    #[arg(long, default_value = "90d")]
    pub since: String,

    /// Number of top hotspots to show.
    #[arg(long = "limit", default_value = "20")]
    pub limit: usize,

    #[command(flatten)]
    pub filter: FilterArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Debug)]
pub struct TrendArgs {
    /// Directories to analyze (defaults to current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Save a new snapshot.
    #[arg(long)]
    pub save: bool,

    /// Label for the snapshot.
    #[arg(long)]
    pub label: Option<String>,

    /// Compare two snapshot references.
    #[arg(long, num_args = 2)]
    pub compare: Option<Vec<String>>,

    /// List all snapshots.
    #[arg(long)]
    pub list: bool,

    #[command(flatten)]
    pub output: OutputArgs,
}

/// Filter options.
#[derive(Args, Debug)]
pub struct FilterArgs {
    /// Only count specified languages (comma-separated).
    #[arg(short, long, value_delimiter = ',')]
    pub lang: Option<Vec<String>>,

    /// Exclude patterns (comma-separated globs).
    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,

    /// Exclude files matching regex pattern.
    #[arg(long)]
    pub exclude_files: Option<String>,

    /// Include only files matching regex pattern.
    #[arg(long)]
    pub include_files: Option<String>,

    /// Minimum line count filter.
    #[arg(long)]
    pub min_lines: Option<usize>,

    /// Maximum line count filter.
    #[arg(long)]
    pub max_lines: Option<usize>,

    /// Maximum directory depth (0 = unlimited).
    #[arg(short, long)]
    pub depth: Option<usize>,

    /// Include all files (including dependencies).
    #[arg(short, long)]
    pub all: bool,

    /// Disable .gitignore rules.
    #[arg(long)]
    pub no_gitignore: bool,

    /// Disable smart directory exclusion.
    #[arg(long)]
    pub no_smart_exclude: bool,
}

/// Output options.
#[derive(Args, Debug)]
pub struct OutputArgs {
    /// Output format.
    #[arg(short, long, value_enum, default_value = "console")]
    pub format: OutputFormatArg,

    /// Output file path.
    #[arg(short = 'O', long)]
    pub output_file: Option<PathBuf>,

    /// Show only summary.
    #[arg(short, long)]
    pub summary: bool,

    /// Quiet mode (no output).
    #[arg(short, long)]
    pub quiet: bool,

    /// Verbose output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Sort order.
    #[arg(long, value_enum, default_value = "lines")]
    pub sort: SortByArg,

    /// Show only top N results.
    #[arg(long)]
    pub top: Option<usize>,
}

/// Advanced options.
#[derive(Args, Debug)]
pub struct AdvancedArgs {
    /// Number of threads (defaults to CPU count).
    #[arg(short = 'j', long)]
    pub threads: Option<usize>,

    /// Configuration file path.
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Don't load configuration files.
    #[arg(long)]
    pub no_config: bool,

    /// Show git repository information.
    #[arg(long)]
    pub git_info: bool,

    /// List supported languages.
    #[arg(long)]
    pub list_languages: bool,
}

/// Output format argument.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormatArg {
    #[default]
    Console,
    Json,
    Csv,
    Markdown,
    Html,
}

/// Sort order argument.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum SortByArg {
    #[default]
    Lines,
    Files,
    Code,
    Name,
    Size,
}

const EXAMPLES: &str = r#"
Examples:
  codelens                        # Analyze current directory
  codelens src tests              # Analyze multiple directories
  codelens -l rust,go             # Only count Rust and Go files
  codelens -f json -O stats.json  # Output JSON to file
  codelens --exclude vendor,dist  # Exclude directories
  codelens --top 20 --sort code   # Show top 20 by code lines
  codelens --git-info             # Include git information
  codelens --list-languages       # List supported languages
  codelens health .               # Code health report
  codelens hotspot . --since 30d  # Change hotspot analysis
  codelens trend --save           # Save trend snapshot
  codelens trend --list           # List snapshots
  codelens trend --compare latest~1 latest  # Compare snapshots
"#;
