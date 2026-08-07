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
