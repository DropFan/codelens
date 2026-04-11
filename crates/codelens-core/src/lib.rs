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
    AnalysisResult, Complexity, FileStats, LanguageSummary, LineStats, RepoStats, RepoSummary,
    SizeDistribution, Summary,
};
pub use config::Config;
pub use error::{Error, Result};
pub use git::{FileChurn, GitClient};
pub use insight::estimation::{
    CocomoBasicModel, CocomoIIModel, CostConfig, EstimationComparison, EstimationModel,
    EstimationReport, LocomoModel, ProjectType, PutnamModel,
};
pub use language::{Language, LanguageRegistry};
pub use output::{OutputFormat, OutputOptions};

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use analyzer::FileAnalyzer;
use filter::FilterChain;
use walker::ParallelWalker;

/// Analyze code statistics for the given paths.
///
/// This is the main entry point for the library.
pub fn analyze<P: AsRef<Path>>(paths: &[P], config: &Config) -> Result<AnalysisResult> {
    let start = Instant::now();

    // Initialize components
    let registry = Arc::new(LanguageRegistry::with_builtin()?);
    let analyzer = Arc::new(FileAnalyzer::new(Arc::clone(&registry), config));
    let filter: Arc<dyn filter::Filter> = Arc::new(FilterChain::new(config)?);
    let walker = ParallelWalker::new(config.walker.clone());

    // Collect all file stats
    let mut all_stats = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;

    for path in paths {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::DirectoryNotFound {
                path: path.to_path_buf(),
            });
        }

        walker.walk_and_analyze(
            path,
            Arc::clone(&analyzer),
            Arc::clone(&filter),
            |stats| {
                all_stats.push(stats);
                scanned_files += 1;
            },
            |_| {
                skipped_files += 1;
            },
        )?;
    }

    // Build summary
    let summary = Summary::from_file_stats(&all_stats);
    let elapsed = start.elapsed();

    Ok(AnalysisResult {
        files: all_stats,
        summary,
        elapsed,
        scanned_files,
        skipped_files,
    })
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
