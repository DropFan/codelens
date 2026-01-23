//! # codelens-core
//!
//! Core library for codelens - a high performance code statistics tool.
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
//!
//! let config = Config::default();
//! let result = analyze(".", &config).unwrap();
//!
//! println!("Total files: {}", result.summary.total_files);
//! println!("Total lines: {}", result.summary.lines.total);
//! ```

pub mod analyzer;
pub mod config;
pub mod error;
pub mod filter;
pub mod language;
pub mod output;
pub mod walker;

pub use analyzer::stats::{
    AnalysisResult, Complexity, FileStats, LanguageSummary, LineStats, RepoStats, RepoSummary,
    SizeDistribution, Summary,
};
pub use config::Config;
pub use error::{Error, Result};
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
