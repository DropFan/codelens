//! Configuration management.

mod file;

pub use file::{load_config_file, PartialConfig};

use crate::walker::WalkerConfig;

/// Main configuration structure.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Walker configuration.
    pub walker: WalkerConfig,
    /// Filter configuration.
    pub filter: FilterConfig,
    /// Output configuration.
    pub output: OutputConfig,
    /// Extra extension → language mappings (`--count-as jsp:html`).
    pub count_as: Vec<(String, String)>,
    /// Custom language definitions TOML loaded on top of the built-in
    /// languages (`--languages-file`).
    pub languages_file: Option<std::path::PathBuf>,
    /// Skip line-level duplication collection (ULOC / duplicate_lines).
    /// The collector keeps every line hash in memory until the walk
    /// finishes, which can cost hundreds of MB on very large codebases;
    /// with this set the metrics report as "not measured" instead.
    pub no_dup_scan: bool,
}

/// Filter configuration.
#[derive(Debug, Clone, Default)]
pub struct FilterConfig {
    /// Glob patterns to exclude.
    pub excludes: Vec<String>,
    /// Glob patterns to include.
    pub includes: Vec<String>,
    /// Regex patterns to exclude files.
    pub exclude_files: Vec<String>,
    /// Regex patterns to include files.
    pub include_files: Vec<String>,
    /// Regex patterns to exclude directories.
    pub exclude_dirs: Vec<String>,
    /// Target languages (empty = all).
    pub languages: Vec<String>,
    /// Minimum line count filter.
    pub min_lines: Option<usize>,
    /// Maximum line count filter.
    pub max_lines: Option<usize>,
    /// Use smart directory exclusion.
    pub smart_exclude: bool,
    /// Include all files (including dependencies).
    pub include_all: bool,
    /// Skip files whose content duplicates an already-seen file.
    pub no_duplicates: bool,
    /// Skip minified/generated files (long average line length or a
    /// generated-code marker in the first line).
    pub no_min_gen: bool,
    /// Ignore .gitattributes linguist attributes (language overrides and
    /// vendored/generated exclusion are honored by default).
    pub no_linguist: bool,
}

impl FilterConfig {
    /// Check if any patterns are configured.
    pub fn has_patterns(&self) -> bool {
        !self.excludes.is_empty()
            || !self.includes.is_empty()
            || !self.exclude_files.is_empty()
            || !self.include_files.is_empty()
            || !self.exclude_dirs.is_empty()
            || !self.languages.is_empty()
    }
}

/// Output configuration.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    /// Output format.
    pub format: OutputFormatType,
    /// Output file path.
    pub file: Option<std::path::PathBuf>,
    /// Show only summary.
    pub summary_only: bool,
    /// Show per-file statistics.
    pub by_file: bool,
    /// Show per-directory statistics (tree view).
    pub by_dir: bool,
    /// Directory depth for --by-dir.
    pub dir_depth: usize,
    /// Show estimated LLM token counts.
    pub show_tokens: bool,
    /// Sort order.
    pub sort_by: SortBy,
    /// Limit results to top N.
    pub top_n: Option<usize>,
    /// Show verbose output.
    pub verbose: bool,
    /// Quiet mode (no output).
    pub quiet: bool,
    /// Show git information.
    pub show_git_info: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormatType::Console,
            file: None,
            summary_only: false,
            by_file: false,
            by_dir: false,
            dir_depth: 3,
            show_tokens: false,
            sort_by: SortBy::Lines,
            top_n: None,
            verbose: false,
            quiet: false,
            show_git_info: false,
        }
    }
}

/// Output format type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormatType {
    #[default]
    Console,
    Json,
    Csv,
    Markdown,
    Html,
    /// Prometheus text exposition format.
    OpenMetrics,
    /// shields.io endpoint badge JSON.
    Badge,
    /// SARIF 2.1.0 (GitHub code scanning / reviewdog).
    Sarif,
}

/// Sort order for results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Lines,
    Files,
    Code,
    Name,
    Size,
}
