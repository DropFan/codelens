//! Directory traversal with parallel processing.

mod parallel;

pub use parallel::{ParallelWalker, SkipReason, WalkerConfig};
