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

    /// Not a git repository.
    #[error("not a git repository: {path}")]
    NotGitRepo { path: PathBuf },

    /// Git command execution failed.
    #[error("git command failed: {message}")]
    GitError { message: String },

    /// No snapshots found.
    #[error("no snapshots found in {path}")]
    NoSnapshots { path: PathBuf },

    /// Snapshot not found.
    #[error("snapshot not found: {id}")]
    SnapshotNotFound { id: String },

    /// Language definition file parse error.
    #[error("failed to parse language definitions")]
    LanguageParse(#[from] toml::de::Error),

    /// Language definition file parse error with the offending path.
    /// Boxed source: keeps the enum below clippy's result_large_err limit.
    #[error("failed to parse language definitions file: {path}")]
    LanguageFileParse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

/// Result type alias for codelens-core operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_display_directory_not_found() {
        let err = Error::DirectoryNotFound {
            path: PathBuf::from("/nonexistent"),
        };
        assert_eq!(err.to_string(), "directory not found: /nonexistent");
    }

    #[test]
    fn test_error_display_invalid_language() {
        let err = Error::InvalidLanguage {
            name: "Test".to_string(),
            reason: "missing extensions".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "invalid language definition 'Test': missing extensions"
        );
    }

    #[test]
    #[allow(clippy::invalid_regex)]
    fn test_error_display_invalid_regex() {
        let regex_err = regex::Regex::new("[invalid").unwrap_err();
        let err = Error::InvalidRegex {
            pattern: "[invalid".to_string(),
            source: regex_err,
        };
        assert!(err.to_string().contains("invalid regex pattern"));
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: Error = Error::OutputWrite(io_err);
        assert!(err.to_string().contains("failed to write output"));
    }

    #[test]
    fn test_error_display_not_git_repo() {
        let err = Error::NotGitRepo {
            path: PathBuf::from("/some/path"),
        };
        assert_eq!(err.to_string(), "not a git repository: /some/path");
    }

    #[test]
    fn test_error_display_git_error() {
        let err = Error::GitError {
            message: "command not found".to_string(),
        };
        assert_eq!(err.to_string(), "git command failed: command not found");
    }

    #[test]
    fn test_error_display_no_snapshots() {
        let err = Error::NoSnapshots {
            path: PathBuf::from(".codelens/snapshots"),
        };
        assert_eq!(err.to_string(), "no snapshots found in .codelens/snapshots");
    }

    #[test]
    fn test_error_display_snapshot_not_found() {
        let err = Error::SnapshotNotFound {
            id: "2026-04-01".to_string(),
        };
        assert_eq!(err.to_string(), "snapshot not found: 2026-04-01");
    }

    #[test]
    fn test_result_type() {
        fn returns_ok() -> Result<i32> {
            Ok(42)
        }
        assert_eq!(returns_ok().unwrap(), 42);

        let err: Result<i32> = Err(Error::DirectoryNotFound {
            path: PathBuf::from("/test"),
        });
        assert!(err.is_err());
    }
}
