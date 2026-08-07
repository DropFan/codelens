//! Configuration file loading.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

use super::{Config, OutputFormatType, SortBy};

/// Partially-specified configuration loaded from a config file.
///
/// Every field is `Option`: `None` means "not specified in the file", so the
/// value can fall back to CLI arguments or built-in defaults during merging.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PartialConfig {
    /// Exclude patterns.
    pub excludes: Option<String>,
    /// Include patterns.
    pub includes: Option<String>,
    /// Exclude file regex.
    pub exclude_files: Option<String>,
    /// Include file regex.
    pub include_files: Option<String>,
    /// Exclude directory regex.
    pub exclude_dirs: Option<String>,
    /// Target languages.
    pub lang: Option<String>,
    /// Extension → language mappings, e.g. "jsp:html,tpl:php".
    pub count_as: Option<String>,
    /// Minimum lines.
    pub min_lines: Option<usize>,
    /// Maximum lines.
    pub max_lines: Option<usize>,
    /// Output format.
    pub output: Option<String>,
    /// Output file.
    pub output_file: Option<String>,
    /// Number of threads.
    pub threads: Option<usize>,
    /// Maximum depth.
    pub depth: Option<usize>,
    /// Sort by.
    pub sort: Option<String>,
    /// Top N results.
    pub top: Option<usize>,
    /// Show git info.
    pub git_info: Option<bool>,
    /// Use gitignore.
    pub no_gitignore: Option<bool>,
    /// Use smart exclude.
    pub no_smart_exclude: Option<bool>,
    /// Parallel processing.
    pub parallel: Option<bool>,
    /// Summary only.
    pub summary: Option<bool>,
    /// Per-file statistics.
    pub by_file: Option<bool>,
    /// Verbose output.
    pub verbose: Option<bool>,
    /// Quiet mode.
    pub quiet: Option<bool>,
}

impl PartialConfig {
    /// Apply every specified (non-`None`) field onto `config`, leaving
    /// unspecified fields untouched.
    pub fn apply_to(&self, config: &mut Config) {
        if let Some(threads) = self.threads {
            config.walker.threads = threads;
        }
        if let Some(no_gitignore) = self.no_gitignore {
            config.walker.use_gitignore = !no_gitignore;
        }
        if let Some(depth) = self.depth {
            config.walker.max_depth = Some(depth);
        }

        if let Some(ref v) = self.excludes {
            config.filter.excludes = parse_comma_list(v);
        }
        if let Some(ref v) = self.includes {
            config.filter.includes = parse_comma_list(v);
        }
        if let Some(ref v) = self.exclude_files {
            config.filter.exclude_files = parse_comma_list(v);
        }
        if let Some(ref v) = self.include_files {
            config.filter.include_files = parse_comma_list(v);
        }
        if let Some(ref v) = self.exclude_dirs {
            config.filter.exclude_dirs = parse_comma_list(v);
        }
        if let Some(ref v) = self.lang {
            config.filter.languages = parse_comma_list(v);
        }
        if let Some(ref v) = self.count_as {
            config.count_as = parse_count_as_list(v);
        }
        if let Some(min_lines) = self.min_lines {
            config.filter.min_lines = Some(min_lines);
        }
        if let Some(max_lines) = self.max_lines {
            config.filter.max_lines = Some(max_lines);
        }
        if let Some(no_smart_exclude) = self.no_smart_exclude {
            config.filter.smart_exclude = !no_smart_exclude;
        }

        if let Some(ref v) = self.output {
            config.output.format = parse_format(v);
        }
        if let Some(ref v) = self.output_file {
            config.output.file = Some(v.into());
        }
        if let Some(ref v) = self.sort {
            config.output.sort_by = parse_sort(v);
        }
        if let Some(top) = self.top {
            config.output.top_n = Some(top);
        }
        if let Some(git_info) = self.git_info {
            config.output.show_git_info = git_info;
        }
        if let Some(summary) = self.summary {
            config.output.summary_only = summary;
        }
        if let Some(by_file) = self.by_file {
            config.output.by_file = by_file;
        }
        if let Some(verbose) = self.verbose {
            config.output.verbose = verbose;
        }
        if let Some(quiet) = self.quiet {
            config.output.quiet = quiet;
        }
    }
}

/// Load a partial configuration from a TOML file.
pub fn load_config_file(path: &Path) -> Result<PartialConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;

    toml::from_str(&content).map_err(|e| Error::ConfigParse {
        path: path.to_path_buf(),
        source: e,
    })
}

fn parse_comma_list(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_string()).collect()
}

/// Parse "jsp:html,tpl:php" into (extension, language) pairs.
fn parse_count_as_list(s: &str) -> Vec<(String, String)> {
    let to_pair = |pair: &str| {
        let (ext, lang) = pair.split_once(':')?;
        Some((ext.trim().to_string(), lang.trim().to_string()))
    };
    s.split(',').filter_map(to_pair).collect()
}

fn parse_format(s: &str) -> OutputFormatType {
    match s {
        "json" => OutputFormatType::Json,
        "csv" => OutputFormatType::Csv,
        "markdown" | "md" => OutputFormatType::Markdown,
        "html" => OutputFormatType::Html,
        "openmetrics" => OutputFormatType::OpenMetrics,
        "badge" => OutputFormatType::Badge,
        _ => OutputFormatType::Console,
    }
}

fn parse_sort(s: &str) -> SortBy {
    match s {
        "files" => SortBy::Files,
        "code" => SortBy::Code,
        "name" => SortBy::Name,
        "size" => SortBy::Size,
        _ => SortBy::Lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
            excludes = "*test*,*mock*"
            lang = "rust,go"
            output = "json"
            threads = 4
            depth = 5
            git_info = true
        "#
        )
        .unwrap();

        let partial = load_config_file(file.path()).unwrap();
        let mut config = Config::default();
        partial.apply_to(&mut config);

        assert_eq!(config.filter.excludes, vec!["*test*", "*mock*"]);
        assert_eq!(config.filter.languages, vec!["rust", "go"]);
        assert_eq!(config.output.format, OutputFormatType::Json);
        assert_eq!(config.walker.threads, 4);
        assert_eq!(config.walker.max_depth, Some(5));
        assert!(config.output.show_git_info);
    }

    #[test]
    fn test_apply_to_leaves_unspecified_fields_untouched() {
        // A config file that only sets `lang` must not reset other fields.
        let partial: PartialConfig = toml::from_str(r#"lang = "rust""#).unwrap();

        let mut config = Config::default();
        config.filter.excludes = vec!["vendor".to_string()];
        config.output.format = OutputFormatType::Html;
        config.walker.threads = 7;

        partial.apply_to(&mut config);

        assert_eq!(config.filter.languages, vec!["rust"]);
        // Untouched by the partial config:
        assert_eq!(config.filter.excludes, vec!["vendor"]);
        assert_eq!(config.output.format, OutputFormatType::Html);
        assert_eq!(config.walker.threads, 7);
    }

    #[test]
    fn test_parse_comma_list() {
        assert_eq!(parse_comma_list("a, b, c"), vec!["a", "b", "c"]);
    }
}
