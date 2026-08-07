//! Parallel directory walker using the `ignore` crate.

use std::path::Path;
use std::sync::Arc;

use crossbeam_channel::bounded;
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::analyzer::stats::FileStats;
use crate::analyzer::FileAnalyzer;
use crate::error::Result;
use crate::filter::Filter;

/// Configuration for the parallel walker.
#[derive(Debug, Clone)]
pub struct WalkerConfig {
    /// Number of threads to use.
    pub threads: usize,
    /// Whether to follow symbolic links.
    pub follow_symlinks: bool,
    /// Whether to respect .gitignore files.
    pub use_gitignore: bool,
    /// Maximum directory depth (None = unlimited).
    pub max_depth: Option<usize>,
    /// Additional ignore FILE NAMES (gitignore syntax files, like
    /// ".myignore") — not patterns themselves.
    pub custom_ignores: Vec<String>,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            threads: num_cpus::get(),
            follow_symlinks: false,
            use_gitignore: true,
            max_depth: None,
            custom_ignores: Vec::new(),
        }
    }
}

/// Parallel directory walker.
pub struct ParallelWalker {
    config: WalkerConfig,
}

impl ParallelWalker {
    /// Create a new parallel walker.
    pub fn new(config: WalkerConfig) -> Self {
        Self { config }
    }

    /// Walk directories and analyze files in parallel.
    ///
    /// Calls `on_file` for each successfully analyzed file,
    /// and `on_skip` (with the reason) for each skipped file.
    pub fn walk_and_analyze<F, S>(
        &self,
        root: &Path,
        analyzer: Arc<FileAnalyzer>,
        filter: Arc<dyn Filter>,
        mut on_file: F,
        mut on_skip: S,
    ) -> Result<()>
    where
        F: FnMut(FileStats) + Send,
        S: FnMut(&Path, SkipReason) + Send,
    {
        let (tx, rx) = bounded::<WalkResult>(1000);

        // Build the walker
        let mut builder = WalkBuilder::new(root);
        builder
            .hidden(false) // Don't skip hidden files by default
            .git_ignore(self.config.use_gitignore)
            .git_global(self.config.use_gitignore)
            .git_exclude(self.config.use_gitignore)
            .follow_links(self.config.follow_symlinks)
            .threads(self.config.threads);

        if let Some(depth) = self.config.max_depth {
            builder.max_depth(Some(depth));
        }

        // Project-level ignore file with gitignore syntax (like .sccignore /
        // .tokeignore), honored in any walked directory
        builder.add_custom_ignore_filename(".codelensignore");

        // Additional ignore file names from configuration
        for filename in &self.config.custom_ignores {
            builder.add_custom_ignore_filename(filename);
        }

        // Start parallel walk with concurrent consumer.
        // The consumer MUST run in parallel with `run()` because `run()` blocks
        // until all walker threads finish. With a bounded channel, walker threads
        // block on send when the channel is full. If the consumer only starts
        // after `run()` returns, this creates a deadlock when results exceed the
        // channel capacity.
        let filter_clone = Arc::clone(&filter);
        let analyzer_clone = Arc::clone(&analyzer);

        std::thread::scope(|s| {
            // Spawn consumer thread that drains results while producers are running
            let consumer = s.spawn(|| {
                for result in &rx {
                    match result {
                        WalkResult::File(stats) => on_file(stats),
                        WalkResult::Skipped(path, reason) => on_skip(&path, reason),
                    }
                }
            });

            // Run producers (blocks until all walker threads finish)
            builder.build_parallel().run(|| {
                let tx = tx.clone();
                let filter = Arc::clone(&filter_clone);
                let analyzer = Arc::clone(&analyzer_clone);
                // Per-thread reusable buffer (64KB initial capacity)
                let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);

                Box::new(move |entry: std::result::Result<DirEntry, ignore::Error>| {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => {
                            // Traversal error (permissions, broken symlink, ...)
                            let _ = tx.send(WalkResult::Skipped(
                                std::path::PathBuf::new(),
                                SkipReason::Error,
                            ));
                            return WalkState::Continue;
                        }
                    };

                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    // Apply custom filter
                    if !filter.should_include(path, is_dir) {
                        if is_dir {
                            return WalkState::Skip;
                        }
                        let _ = tx.send(WalkResult::Skipped(
                            path.to_path_buf(),
                            SkipReason::Filtered,
                        ));
                        return WalkState::Continue;
                    }

                    // Skip directories (they're handled by the walker)
                    if is_dir {
                        return WalkState::Continue;
                    }

                    // Skip non-files
                    if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        return WalkState::Continue;
                    }

                    // Read file into reusable buffer
                    buf.clear();
                    match std::fs::File::open(path).and_then(|mut f| {
                        if let Ok(meta) = f.metadata() {
                            buf.reserve(meta.len() as usize);
                        }
                        std::io::Read::read_to_end(&mut f, &mut buf)
                    }) {
                        Ok(_) => match analyzer.analyze_from_bytes(path, &buf) {
                            Ok(Some(stats)) => {
                                let _ = tx.send(WalkResult::File(stats));
                            }
                            Ok(None) => {
                                // Unrecognized language, binary, or line filter
                                let _ = tx.send(WalkResult::Skipped(
                                    path.to_path_buf(),
                                    SkipReason::Filtered,
                                ));
                            }
                            Err(_) => {
                                let _ = tx.send(WalkResult::Skipped(
                                    path.to_path_buf(),
                                    SkipReason::Error,
                                ));
                            }
                        },
                        Err(_) => {
                            // Read failure (permissions, vanished file, ...)
                            let _ =
                                tx.send(WalkResult::Skipped(path.to_path_buf(), SkipReason::Error));
                        }
                    }

                    // Shrink buffer if it grew too large from a single file
                    if buf.capacity() > 1_048_576 {
                        buf = Vec::with_capacity(64 * 1024);
                    }

                    WalkState::Continue
                })
            });

            // Close the sender so consumer finishes when all results are drained
            drop(tx);

            // Wait for consumer to finish
            consumer.join().expect("consumer thread panicked");
        });

        Ok(())
    }
}

impl Default for ParallelWalker {
    fn default() -> Self {
        Self::new(WalkerConfig::default())
    }
}

/// Why a file was skipped during the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Excluded by filters, unrecognized language, or binary content.
    Filtered,
    /// Read, traversal, or analysis failure — the file SHOULD have been
    /// counted but couldn't be.
    Error,
}

/// Result from walking a single entry.
enum WalkResult {
    /// Successfully analyzed file.
    File(FileStats),
    /// Skipped file with the reason.
    Skipped(std::path::PathBuf, SkipReason),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    struct AllowAll;
    impl Filter for AllowAll {
        fn should_include(&self, _path: &Path, _is_dir: bool) -> bool {
            true
        }
    }

    #[test]
    fn test_walker_config_default() {
        let config = WalkerConfig::default();
        assert!(config.threads > 0);
        assert!(config.use_gitignore);
        assert!(!config.follow_symlinks);
    }

    #[test]
    fn test_walk_empty_dir() {
        let dir = TempDir::new().unwrap();
        let walker = ParallelWalker::default();
        let registry = Arc::new(crate::language::LanguageRegistry::empty());
        let analyzer = Arc::new(FileAnalyzer::new(
            registry,
            &crate::config::Config::default(),
        ));
        let filter = Arc::new(AllowAll);

        let count = AtomicUsize::new(0);
        walker
            .walk_and_analyze(
                dir.path(),
                analyzer,
                filter,
                |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                },
                |_, _| {},
            )
            .unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_walk_many_files_no_deadlock() {
        // Regression test: >1000 files previously caused deadlock because
        // the bounded channel (capacity 1000) filled up while run() blocked
        // waiting for threads, and the consumer only started after run().
        let dir = TempDir::new().unwrap();
        let file_count = 1500;
        for i in 0..file_count {
            std::fs::write(dir.path().join(format!("file_{i}.rs")), "fn main() {}\n").unwrap();
        }

        let walker = ParallelWalker::default();
        let registry = Arc::new(crate::language::LanguageRegistry::with_builtin().unwrap());
        let analyzer = Arc::new(FileAnalyzer::new(
            registry,
            &crate::config::Config::default(),
        ));
        let filter = Arc::new(AllowAll);

        let analyzed = AtomicUsize::new(0);
        walker
            .walk_and_analyze(
                dir.path(),
                analyzer,
                filter,
                |_| {
                    analyzed.fetch_add(1, Ordering::SeqCst);
                },
                |_, _| {},
            )
            .unwrap();

        assert_eq!(analyzed.load(Ordering::SeqCst), file_count);
    }
}
