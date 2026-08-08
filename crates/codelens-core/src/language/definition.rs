//! Language definition structures.

use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

use crate::analyzer::trie::{self, TokenTrie};

/// Definition of a programming language.
#[derive(Debug, Deserialize)]
pub struct Language {
    /// Language display name (e.g., "Rust", "Python").
    pub name: String,

    /// File extensions (e.g., [".rs", ".rlib"]).
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Special filenames (e.g., ["Makefile", "Dockerfile"]).
    #[serde(default)]
    pub filenames: Vec<String>,

    /// Single-line comment prefixes (e.g., ["//", "#"]).
    #[serde(default)]
    pub line_comments: Vec<String>,

    /// Block comment delimiters (e.g., [("/*", "*/")]).
    #[serde(default, deserialize_with = "deserialize_block_comments")]
    pub block_comments: Vec<(String, String)>,

    /// String literal delimiters for accurate parsing.
    #[serde(default)]
    pub string_delimiters: Vec<StringDelimiter>,

    /// Regex pattern to match function definitions.
    #[serde(default)]
    pub function_pattern: Option<String>,

    /// Keywords that contribute to cyclomatic complexity.
    #[serde(default)]
    pub complexity_keywords: Vec<String>,

    /// Whether block comments can be nested (e.g., Rust allows /* /* */ */).
    #[serde(default)]
    pub nested_comments: bool,

    /// Lazily-built token trie and process mask.
    #[serde(skip)]
    pub(crate) tokens_cache: OnceLock<(TokenTrie, u8)>,

    /// Lazily-compiled complexity regex patterns.
    #[serde(skip)]
    pub(crate) complexity_cache: OnceLock<ComplexityPatterns>,
}

/// Precompiled regex patterns for complexity analysis.
#[derive(Debug)]
pub struct ComplexityPatterns {
    /// Compiled function_pattern regex.
    pub function_re: Option<Regex>,
    /// Single alternation regex matching all complexity keywords: `\b(if|else|...)\b`
    pub keywords_re: Option<Regex>,
}

impl Clone for Language {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            extensions: self.extensions.clone(),
            filenames: self.filenames.clone(),
            line_comments: self.line_comments.clone(),
            block_comments: self.block_comments.clone(),
            string_delimiters: self.string_delimiters.clone(),
            function_pattern: self.function_pattern.clone(),
            complexity_keywords: self.complexity_keywords.clone(),
            nested_comments: self.nested_comments,
            tokens_cache: OnceLock::new(),
            complexity_cache: OnceLock::new(),
        }
    }
}

impl Language {
    /// Get the token trie and process mask, building them on first access.
    pub fn tokens(&self) -> &(TokenTrie, u8) {
        self.tokens_cache
            .get_or_init(|| trie::build_from_language(self))
    }

    /// Get precompiled complexity regex patterns, building them on first access.
    pub fn complexity_patterns(&self) -> &ComplexityPatterns {
        self.complexity_cache.get_or_init(|| {
            let function_re = self
                .function_pattern
                .as_ref()
                .and_then(|p| Regex::new(p).ok());

            let keywords_re = keywords_pattern(&self.complexity_keywords)
                .and_then(|pattern| Regex::new(&pattern).ok());

            ComplexityPatterns {
                function_re,
                keywords_re,
            }
        })
    }
}

/// Build the alternation pattern matching all complexity keywords:
/// `\b(if|else|...)\b`. Shared by the lazy compilation above and the
/// load-time validation of user language files so both compile exactly
/// the same regex.
pub(crate) fn keywords_pattern(keywords: &[String]) -> Option<String> {
    if keywords.is_empty() {
        return None;
    }
    let alts: Vec<String> = keywords.iter().map(|k| regex::escape(k)).collect();
    Some(format!(r"\b({})\b", alts.join("|")))
}

impl Default for Language {
    fn default() -> Self {
        Self {
            name: "Unknown".to_string(),
            extensions: vec![],
            filenames: vec![],
            line_comments: vec![],
            block_comments: vec![],
            string_delimiters: vec![],
            function_pattern: None,
            complexity_keywords: vec![],
            nested_comments: false,
            tokens_cache: OnceLock::new(),
            complexity_cache: OnceLock::new(),
        }
    }
}

/// String delimiter definition.
#[derive(Debug, Clone, Deserialize)]
pub struct StringDelimiter {
    /// Opening delimiter.
    pub start: String,
    /// Closing delimiter.
    pub end: String,
    /// Escape character (if any).
    #[serde(default)]
    pub escape: Option<String>,
    /// Whether the string literal may span multiple lines
    /// (e.g., Rust `"..."`, JS backtick, Go backtick).
    #[serde(default)]
    pub multiline: bool,
}

/// Custom deserializer for block comments that handles TOML array format.
fn deserialize_block_comments<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<Vec<String>> = Vec::deserialize(deserializer)?;
    let to_pair = |pair: Vec<String>| match &pair[..] {
        [open, close, ..] => Some((open.clone(), close.clone())),
        _ => None,
    };
    Ok(raw.into_iter().filter_map(to_pair).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_default() {
        let lang = Language::default();
        assert_eq!(lang.name, "Unknown");
        assert!(lang.extensions.is_empty());
        assert!(lang.line_comments.is_empty());
    }

    #[test]
    fn test_language_deserialize() {
        let toml = r#"
            name = "Rust"
            extensions = [".rs"]
            line_comments = ["//"]
            block_comments = [["/*", "*/"]]
            function_pattern = "fn\\s+\\w+"
            complexity_keywords = ["if", "for", "while"]
            nested_comments = true
        "#;

        let lang: Language = toml::from_str(toml).unwrap();
        assert_eq!(lang.name, "Rust");
        assert_eq!(lang.extensions, vec![".rs"]);
        assert_eq!(lang.line_comments, vec!["//"]);
        assert_eq!(
            lang.block_comments,
            vec![("/*".to_string(), "*/".to_string())]
        );
        assert!(lang.nested_comments);
    }
}
