//! Language definition and detection.

mod definition;
mod linguist;
mod registry;

pub use definition::{Language, StringDelimiter};
pub use linguist::LinguistAttributes;
pub use registry::LanguageRegistry;
