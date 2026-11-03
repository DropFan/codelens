//! # codelens-core
//!
//! Core library for codelens - a high performance code analysis tool.
//!
//! ## Features
//!
//! - Fast parallel file traversal using `ignore` crate
//! - Support for 70+ programming languages
//! - Accurate line counting (code, comments, blanks)
//! - Cyclomatic complexity analysis
//! - Multiple output formats (Console, JSON, CSV, Markdown, HTML)
//! - Respects `.gitignore` rules
//! - Smart directory exclusion based on project type
//!
//! ## Example
//!
//! ```rust,no_run
//! use codelens_core::{analyze, Config};
//! use std::path::PathBuf;
//!
//! let config = Config::default();
//! let paths = vec![PathBuf::from(".")];
//! let result = analyze(&paths, &config).unwrap();
//!
//! println!("Total files: {}", result.summary.total_files);
//! println!("Total lines: {}", result.summary.lines.total);
//! ```

pub mod analyzer;
pub mod config;
pub mod error;
pub mod filter;
pub mod git;
pub mod insight;
pub mod language;
pub mod output;
pub mod walker;

pub use analyzer::stats::{
    AnalysisResult, Complexity, FileStats, HealthRegions, LanguageSummary, LineStats, RegionStats,
    RepoStats, RepoSummary, SizeDistribution, Summary,
};
pub use analyzer::{ComplexityAnalyzer, FunctionSpan};
pub use config::Config;
pub use error::{Error, Result};
pub use git::{CommitRecord, FileChange, FileChurn, GitClient};
pub use insight::estimation::{
    CocomoBasicModel, CocomoIIModel, CostConfig, EstimationComparison, EstimationModel,
    EstimationReport, LocomoModel, ProjectType, PutnamModel,
};
pub use language::{Language, LanguageRegistry};
pub use output::{OutputFormat, OutputOptions};

use std::path::Path;

/// Build the language registry a `Config` describes: builtin languages,
/// then the custom definitions file, then the extension remappings.
///
/// Every consumer that detects languages on behalf of an analysis must go
/// through this — a bare `LanguageRegistry::with_builtin()` would silently
/// ignore `--languages-file` and `--count-as`.
pub fn build_registry(config: &Config) -> Result<LanguageRegistry> {
    let mut registry = LanguageRegistry::with_builtin()?;
    // Custom definitions load before --count-as so the extension
    // mappings can reference custom language names.
    if let Some(ref path) = config.languages_file {
        registry.load_file(path)?;
    }
    for (ext, lang) in &config.count_as {
        registry.map_extension(ext, lang)?;
    }
    Ok(registry)
}

/// Analyze code statistics for the given paths.
///
/// This is the main entry point for the library.
pub fn analyze<P: AsRef<Path>>(paths: &[P], config: &Config) -> Result<AnalysisResult> {
    analyzer::analyze_paths(paths, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_analyze_empty_directory() {
        let dir = TempDir::new().unwrap();
        let config = Config::default();

        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 0);
        assert_eq!(result.summary.lines.total, 0);
    }

    #[test]
    fn test_analyze_single_rust_file() {
        let dir = TempDir::new().unwrap();
        let rust_code = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
        create_test_file(dir.path(), "main.rs", rust_code);

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 1);
        assert!(result.summary.by_language.contains_key("Rust"));
        assert_eq!(result.summary.lines.total, 3);
        assert_eq!(result.summary.lines.code, 3);
    }

    #[test]
    fn test_analyze_multiple_languages() {
        let dir = TempDir::new().unwrap();

        let rust_code = "fn main() {}\n";
        let python_code = "def main():\n    pass\n";
        let js_code = "function main() {}\n";

        create_test_file(dir.path(), "main.rs", rust_code);
        create_test_file(dir.path(), "main.py", python_code);
        create_test_file(dir.path(), "main.js", js_code);

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 3);
        assert!(result.summary.by_language.contains_key("Rust"));
        assert!(result.summary.by_language.contains_key("Python"));
        assert!(result.summary.by_language.contains_key("JavaScript"));
    }

    #[test]
    fn test_analyze_with_comments() {
        let dir = TempDir::new().unwrap();
        let rust_code = r#"// This is a comment
fn main() {
    /* block comment */
    println!("Hello");
}
"#;
        create_test_file(dir.path(), "main.rs", rust_code);

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 1);
        assert!(result.summary.lines.comment > 0);
        assert!(result.summary.lines.code > 0);
    }

    #[test]
    fn test_analyze_nested_directories() {
        let dir = TempDir::new().unwrap();

        create_test_file(dir.path(), "src/main.rs", "fn main() {}\n");
        create_test_file(dir.path(), "src/lib.rs", "pub fn lib() {}\n");
        create_test_file(dir.path(), "tests/test.rs", "#[test]\nfn test() {}\n");

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 3);
    }

    #[test]
    fn test_analyze_nonexistent_path() {
        let config = Config::default();
        let result = analyze(&[PathBuf::from("/nonexistent/path")], &config);

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::DirectoryNotFound { path } => {
                assert_eq!(path, PathBuf::from("/nonexistent/path"));
            }
            _ => panic!("Expected DirectoryNotFound error"),
        }
    }

    #[test]
    fn test_analyze_multiple_paths() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        create_test_file(dir1.path(), "a.rs", "fn a() {}\n");
        create_test_file(dir2.path(), "b.rs", "fn b() {}\n");

        let config = Config::default();
        let result = analyze(&[dir1.path(), dir2.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 2);
    }

    #[test]
    fn test_analyze_respects_gitignore() {
        let dir = TempDir::new().unwrap();

        // Initialize as git repo (required for .gitignore to work)
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();

        // Create .gitignore
        create_test_file(dir.path(), ".gitignore", "ignored/\n");

        // Create files
        create_test_file(dir.path(), "main.rs", "fn main() {}\n");
        create_test_file(dir.path(), "ignored/skip.rs", "fn skip() {}\n");

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        // Should only count main.rs, not ignored/skip.rs
        assert_eq!(result.summary.total_files, 1);
    }

    #[test]
    fn test_analyze_result_contains_elapsed_time() {
        let dir = TempDir::new().unwrap();
        create_test_file(dir.path(), "main.rs", "fn main() {}\n");

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        // Elapsed time should be > 0
        assert!(result.elapsed.as_nanos() > 0);
    }

    #[test]
    fn test_analyze_no_dup_scan_skips_duplication_collection() {
        let dir = TempDir::new().unwrap();
        // Two files sharing a long duplicated line.
        let content = "fn duplicated_line_content_here() { body(); }\n";
        create_test_file(dir.path(), "a.rs", content);
        create_test_file(dir.path(), "b.rs", content);

        let config = Config {
            no_dup_scan: true,
            ..Config::default()
        };
        let result = analyze(&[dir.path()], &config).unwrap();

        assert!(!result.summary.dup_scanned);
        assert_eq!(result.summary.uloc, 0, "ULOC must stay unmeasured");
        assert_eq!(result.summary.duplicate_lines, 0);
        assert!(result.files.iter().all(|f| f.duplicate_lines == 0));

        // Control: the default config measures the duplication.
        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();
        assert!(result.summary.dup_scanned);
        assert!(result.summary.uloc > 0);
        assert!(result.summary.duplicate_lines > 0);
    }

    #[test]
    fn test_analyze_deduplicates_lexically_equivalent_roots() {
        let dir = TempDir::new().unwrap();
        create_test_file(
            dir.path(),
            "src/lib.rs",
            "fn only_line_in_repo() { compute_something(); }\n",
        );
        let root = dir.path().join("src");
        let dotted_root = dir.path().join("./src");

        let result = analyze(&[root, dotted_root], &Config::default()).unwrap();

        assert_eq!(result.scanned_files, 1);
        assert_eq!(result.summary.total_files, 1);
        assert_eq!(result.files[0].duplicate_lines, 0);
    }

    #[test]
    fn test_analyze_collects_inline_test_health_regions() {
        let dir = TempDir::new().unwrap();
        create_test_file(
            dir.path(),
            "lib.rs",
            r#"fn production() {
    let duplicated_value = compute_something();
}

#[cfg(test)]
mod tests {
    #[test]
    fn works() {
        let duplicated_value = compute_something();
    }
}
"#,
        );

        let result = analyze(&[dir.path()], &Config::default()).unwrap();

        assert!(result.summary.inline_test_regions_scanned);
        assert_eq!(result.summary.health_regions.len(), 1);
        let regions = result.summary.health_regions.values().next().unwrap();
        assert_eq!(regions.production.complexity.functions, 1);
        assert_eq!(regions.tests.complexity.functions, 1);
        assert!(regions.production.lines.code < result.files[0].lines.code);
        assert!(regions.tests.lines.code > 0);
        assert_eq!(regions.production.duplicate_lines, 1);
        assert_eq!(regions.tests.duplicate_lines, 1);
        assert_eq!(result.files[0].duplicate_lines, 2);
    }

    #[test]
    fn test_path_level_tests_are_not_split_again() {
        let dir = TempDir::new().unwrap();
        create_test_file(
            dir.path(),
            "tests/integration.rs",
            "#[test]\nfn works() { assert!(true); }\n",
        );

        let result = analyze(&[dir.path()], &Config::default()).unwrap();

        assert!(result.summary.inline_test_regions_scanned);
        assert!(result.summary.health_regions.is_empty());
    }

    #[test]
    fn test_analyze_with_custom_languages_file() {
        // The definitions file lives OUTSIDE the analyzed tree: .toml is a
        // built-in language and would pollute total_files otherwise.
        let lang_dir = TempDir::new().unwrap();
        let langs_path = lang_dir.path().join("custom.toml");
        fs::write(
            &langs_path,
            r##"
            [mylang]
            name = "MyLang"
            extensions = [".myl"]
            line_comments = ["#"]
            "##,
        )
        .unwrap();

        let dir = TempDir::new().unwrap();
        create_test_file(dir.path(), "hello.myl", "# a comment\ncode line\n");
        // --count-as must be able to reference the custom language name,
        // which requires the definitions to load before the mappings.
        create_test_file(dir.path(), "extra.myx", "more code\n");

        let config = Config {
            languages_file: Some(langs_path),
            count_as: vec![("myx".to_string(), "MyLang".to_string())],
            ..Config::default()
        };
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.summary.total_files, 2);
        let mylang = &result.summary.by_language["MyLang"];
        assert_eq!(mylang.files, 2);
        assert_eq!(mylang.lines.comment, 1);
        assert_eq!(mylang.lines.code, 2);
    }

    #[test]
    fn test_analyze_missing_languages_file_is_hard_error() {
        let dir = TempDir::new().unwrap();
        let config = Config {
            languages_file: Some(PathBuf::from("/nonexistent/langs.toml")),
            ..Config::default()
        };

        let err = analyze(&[dir.path()], &config).unwrap_err();
        assert!(
            err.to_string().contains("/nonexistent/langs.toml"),
            "error must name the offending path: {err}"
        );
    }

    #[test]
    fn test_analyze_invalid_languages_file_error_names_path() {
        let lang_dir = TempDir::new().unwrap();
        let langs_path = lang_dir.path().join("broken.toml");
        fs::write(&langs_path, "not = [ valid toml").unwrap();

        let dir = TempDir::new().unwrap();
        let config = Config {
            languages_file: Some(langs_path.clone()),
            ..Config::default()
        };

        let err = analyze(&[dir.path()], &config).unwrap_err();
        assert!(
            err.to_string().contains(&langs_path.display().to_string()),
            "error must name the offending path: {err}"
        );
    }

    #[test]
    fn test_analyze_scanned_files_count() {
        let dir = TempDir::new().unwrap();

        create_test_file(dir.path(), "a.rs", "fn a() {}\n");
        create_test_file(dir.path(), "b.rs", "fn b() {}\n");
        create_test_file(dir.path(), "c.txt", "not code\n"); // will be skipped

        let config = Config::default();
        let result = analyze(&[dir.path()], &config).unwrap();

        assert_eq!(result.scanned_files, 2);
        assert!(result.skipped_files >= 1); // at least c.txt
    }
}
