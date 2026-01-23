//! File and directory filtering.

mod chain;
mod pattern;
mod smart;

pub use chain::FilterChain;
pub use pattern::PatternFilter;
pub use smart::SmartExclude;

use std::path::Path;

/// Trait for filtering files and directories.
pub trait Filter: Send + Sync {
    /// Check if the path should be included.
    fn should_include(&self, path: &Path, is_dir: bool) -> bool;
}
