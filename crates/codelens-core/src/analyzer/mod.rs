//! Code analysis modules.

pub mod counter;
mod complexity;
mod file;
pub mod stats;
pub mod trie;

pub use complexity::ComplexityAnalyzer;
pub use file::FileAnalyzer;
