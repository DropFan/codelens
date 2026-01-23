//! Error types for codelens-core.

use std::path::PathBuf;
use thiserror::Error;

/// Error type for codelens-core operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Failed to read a file.
    #[error("failed to read file: {path}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse configuration file.
    #[error("failed to parse config file: {path}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    /// Invalid language definition.
    #[error("invalid language definition '{name}': {reason}")]
    InvalidLanguage { name: String, reason: String },

    /// Invalid regex pattern.
    #[error("invalid regex pattern: {pattern}")]
    InvalidRegex {
        pattern: String,
        #[source]
        source: regex::Error,
    },

    /// Invalid glob pattern.
    #[error("invalid glob pattern: {pattern}")]
    InvalidGlob {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    /// Directory not found.
    #[error("directory not found: {path}")]
    DirectoryNotFound { path: PathBuf },

    /// Output write error.
    #[error("failed to write output")]
    OutputWrite(#[from] std::io::Error),

    /// Template render error.
    #[error("failed to render template")]
    TemplateRender(#[from] askama::Error),

    /// JSON serialization error.
    #[error("failed to serialize JSON")]
    JsonSerialize(#[from] serde_json::Error),

    /// Directory traversal error.
    #[error("directory traversal error: {0}")]
    Walk(#[from] ignore::Error),

    /// Language definition file parse error.
    #[error("failed to parse language definitions")]
    LanguageParse(#[from] toml::de::Error),
}

/// Result type alias for codelens-core operations.
pub type Result<T> = std::result::Result<T, Error>;
