//! Code analysis modules.

mod complexity;
pub mod counter;
pub mod duplication;
mod file;
pub mod stats;
pub mod test_code;
pub mod tokens;
pub mod trie;

pub use complexity::{ComplexityAnalyzer, FunctionSpan};
pub use file::FileAnalyzer;
