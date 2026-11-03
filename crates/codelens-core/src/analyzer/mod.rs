//! Code analysis modules.

use std::path::{Component, Path, PathBuf};

mod complexity;
pub mod counter;
pub mod duplication;
mod file;
mod measurements;
mod pipeline;
pub mod stats;
pub mod test_code;
pub mod tokens;
pub mod trie;

pub use complexity::{ComplexityAnalyzer, FunctionSpan};
pub use file::FileAnalyzer;
pub(crate) use pipeline::analyze_paths;

/// Normalize purely lexical path noise without resolving symlinks.
///
/// Walks rooted at `src` and `./src` must identify the same file, while
/// `canonicalize` would change path identity and require the target to exist.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| component.as_os_str())
        .collect()
}
