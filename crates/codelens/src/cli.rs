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

    /// Show top N worst files/directories.
    #[arg(long, default_value = "10")]
    pub top: usize,

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

    /// Show top N hotspots.
    #[arg(long, default_value = "20")]
    pub top: usize,

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

    /// Compare two snapshot references (e.g. "latest~1 latest", "2026-04-01 latest").
    #[arg(long, num_args = 2, value_names = ["FROM", "TO"])]
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

Health (code health score):
  codelens health .               # Health report for current directory
  codelens health src -f json     # Health report in JSON format
  codelens health . --top 20      # Show top 20 worst files/directories

Hotspot (churn x complexity):
  codelens hotspot .              # Hotspots in last 90 days (default)
  codelens hotspot . --since 30d  # Hotspots in last 30 days
  codelens hotspot . --since 6m   # Hotspots in last 6 months
  codelens hotspot . --top 5      # Show top 5 hotspots

Trend (snapshot comparison):
  codelens trend --save           # Save a snapshot
  codelens trend --save --label v1.0  # Save with label
  codelens trend                  # Compare latest two snapshots
  codelens trend --list           # List all snapshots
  codelens trend --compare latest~2 latest  # Compare specific snapshots
"#;
