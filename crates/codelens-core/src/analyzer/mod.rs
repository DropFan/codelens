//! Code analysis modules.

mod complexity;
pub mod counter;
mod file;
pub mod stats;
pub mod trie;

pub use complexity::ComplexityAnalyzer;
pub use file::FileAnalyzer;
