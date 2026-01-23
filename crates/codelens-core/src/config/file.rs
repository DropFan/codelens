//! Configuration file loading.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

use super::{Config, FilterConfig, OutputConfig, OutputFormatType, SortBy};
use crate::walker::WalkerConfig;

/// Configuration file structure.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    /// Exclude patterns.
    excludes: Option<String>,
    /// Include patterns.
    includes: Option<String>,
    /// Exclude file regex.
    exclude_files: Option<String>,
    /// Include file regex.
    include_files: Option<String>,
    /// Exclude directory regex.
    exclude_dirs: Option<String>,
    /// Target languages.
    lang: Option<String>,
    /// Minimum lines.
    min_lines: Option<usize>,
    /// Maximum lines.
    max_lines: Option<usize>,
    /// Output format.
    output: Option<String>,
    /// Output file.
    output_file: Option<String>,
    /// Number of threads.
    threads: Option<usize>,
    /// Maximum depth.
    depth: Option<usize>,
    /// Sort by.
    sort: Option<String>,
    /// Top N results.
    top: Option<usize>,
    /// Show git info.
    git_info: Option<bool>,
    /// Use gitignore.
    no_gitignore: Option<bool>,
    /// Use smart exclude.
    no_smart_exclude: Option<bool>,
    /// Parallel processing.
    parallel: Option<bool>,
    /// Summary only.
    summary: Option<bool>,
    /// Verbose output.
    verbose: Option<bool>,
    /// Quiet mode.
    quiet: Option<bool>,
}

/// Load configuration from a TOML file.
pub fn load_config_file(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;

    let file: ConfigFile = toml::from_str(&content).map_err(|e| Error::ConfigParse {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(Config {
        walker: WalkerConfig {
            threads: file.threads.unwrap_or_else(num_cpus::get),
            use_gitignore: !file.no_gitignore.unwrap_or(false),
            max_depth: file.depth,
            ..Default::default()
        },
        filter: FilterConfig {
            excludes: parse_comma_list(&file.excludes),
            includes: parse_comma_list(&file.includes),
            exclude_files: parse_comma_list(&file.exclude_files),
            include_files: parse_comma_list(&file.include_files),
            exclude_dirs: parse_comma_list(&file.exclude_dirs),
            languages: parse_comma_list(&file.lang),
            min_lines: file.min_lines,
            max_lines: file.max_lines,
            smart_exclude: !file.no_smart_exclude.unwrap_or(false),
            ..Default::default()
        },
        output: OutputConfig {
            format: parse_format(&file.output),
            file: file.output_file.map(Into::into),
            sort_by: parse_sort(&file.sort),
            top_n: file.top,
            show_git_info: file.git_info.unwrap_or(false),
            summary_only: file.summary.unwrap_or(false),
            verbose: file.verbose.unwrap_or(false),
            quiet: file.quiet.unwrap_or(false),
        },
    })
}

fn parse_comma_list(s: &Option<String>) -> Vec<String> {
    s.as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default()
}

fn parse_format(s: &Option<String>) -> OutputFormatType {
    match s.as_deref() {
        Some("json") => OutputFormatType::Json,
        Some("csv") => OutputFormatType::Csv,
        Some("markdown") | Some("md") => OutputFormatType::Markdown,
        Some("html") => OutputFormatType::Html,
        _ => OutputFormatType::Console,
    }
}

fn parse_sort(s: &Option<String>) -> SortBy {
    match s.as_deref() {
        Some("files") => SortBy::Files,
        Some("code") => SortBy::Code,
        Some("name") => SortBy::Name,
        Some("size") => SortBy::Size,
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

        let config = load_config_file(file.path()).unwrap();
        assert_eq!(config.filter.excludes, vec!["*test*", "*mock*"]);
        assert_eq!(config.filter.languages, vec!["rust", "go"]);
        assert_eq!(config.output.format, OutputFormatType::Json);
        assert_eq!(config.walker.threads, 4);
        assert_eq!(config.walker.max_depth, Some(5));
        assert!(config.output.show_git_info);
    }

    #[test]
    fn test_parse_comma_list() {
        assert_eq!(
            parse_comma_list(&Some("a, b, c".to_string())),
            vec!["a", "b", "c"]
        );
        assert!(parse_comma_list(&None).is_empty());
    }
}
