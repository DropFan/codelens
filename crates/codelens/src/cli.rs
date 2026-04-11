//! CLI argument definitions.

use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

/// High performance code analysis tool.
#[derive(Parser, Debug)]
#[command(
    name = "codelens",
    author,
    version,
    about = "High performance code analysis tool — stats, health scores, hotspots, and trends",
    styles = STYLES,
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
    #[command(long_about = "Score code health across five dimensions: complexity, function size, \
        comment ratio, file size, and nesting depth. Reports at project, directory, and file \
        levels with grades from A (best) to F (worst). Use --top to control how many items \
        are shown per level.")]
    Health(HealthArgs),
    /// Detect change hotspots (churn x complexity).
    #[command(long_about = "Find the riskiest files in your codebase by combining git change \
        frequency (churn) with code complexity. Files that change often AND are complex are \
        the most likely sources of bugs. Requires a git repository.")]
    Hotspot(HotspotArgs),
    /// Track codebase trends with snapshots.
    #[command(long_about = "Save snapshots of codebase metrics and compare them over time. \
        Snapshots are stored in .codelens/snapshots/ as JSON files. References: \"latest\", \
        \"latest~N\" (Nth before latest), or date prefix like \"2025-01-01\".")]
    Trend(TrendArgs),
}

#[derive(Args, Debug)]
pub struct HealthArgs {
    /// Directories to analyze (defaults to current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

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

// ANSI: \x1b[1;32m = bold green, \x1b[1;36m = bold cyan, \x1b[2m = dim, \x1b[0m = reset
const EXAMPLES: &str = "\
\x1b[1;32mExamples:\x1b[0m
  \x1b[1;36mcodelens\x1b[0m                        \x1b[2m# Analyze current directory\x1b[0m
  \x1b[1;36mcodelens src tests\x1b[0m              \x1b[2m# Analyze multiple directories\x1b[0m
  \x1b[1;36mcodelens -l rust,go\x1b[0m             \x1b[2m# Only count Rust and Go files\x1b[0m
  \x1b[1;36mcodelens -f json -O stats.json\x1b[0m  \x1b[2m# Output JSON to file\x1b[0m
  \x1b[1;36mcodelens --exclude vendor,dist\x1b[0m  \x1b[2m# Exclude directories\x1b[0m
  \x1b[1;36mcodelens --top 20 --sort code\x1b[0m   \x1b[2m# Show top 20 by code lines\x1b[0m
  \x1b[1;36mcodelens --git-info\x1b[0m             \x1b[2m# Include git information\x1b[0m
  \x1b[1;36mcodelens --list-languages\x1b[0m       \x1b[2m# List supported languages\x1b[0m

\x1b[1;32mHealth\x1b[0m \x1b[2m(code health score):\x1b[0m
  \x1b[1;36mcodelens health .\x1b[0m               \x1b[2m# Health report for current directory\x1b[0m
  \x1b[1;36mcodelens health src -f json\x1b[0m     \x1b[2m# Health report in JSON format\x1b[0m
  \x1b[1;36mcodelens health . --top 20\x1b[0m      \x1b[2m# Show top 20 worst files/directories\x1b[0m

\x1b[1;32mHotspot\x1b[0m \x1b[2m(churn x complexity):\x1b[0m
  \x1b[1;36mcodelens hotspot .\x1b[0m              \x1b[2m# Hotspots in last 90 days (default)\x1b[0m
  \x1b[1;36mcodelens hotspot . --since 30d\x1b[0m  \x1b[2m# Hotspots in last 30 days\x1b[0m
  \x1b[1;36mcodelens hotspot . --since 6m\x1b[0m   \x1b[2m# Hotspots in last 6 months\x1b[0m
  \x1b[1;36mcodelens hotspot . --top 5\x1b[0m      \x1b[2m# Show top 5 hotspots\x1b[0m

\x1b[1;32mTrend\x1b[0m \x1b[2m(snapshot comparison):\x1b[0m
  \x1b[1;36mcodelens trend --save\x1b[0m           \x1b[2m# Save a snapshot\x1b[0m
  \x1b[1;36mcodelens trend --save --label v1.0\x1b[0m  \x1b[2m# Save with label\x1b[0m
  \x1b[1;36mcodelens trend\x1b[0m                  \x1b[2m# Compare latest two snapshots\x1b[0m
  \x1b[1;36mcodelens trend --list\x1b[0m           \x1b[2m# List all snapshots\x1b[0m
  \x1b[1;36mcodelens trend --compare latest~2 latest\x1b[0m  \x1b[2m# Compare specific snapshots\x1b[0m
";
