//! Language definition structures.

use serde::Deserialize;

/// Definition of a programming language.
#[derive(Debug, Clone, Deserialize)]
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
}

/// Custom deserializer for block comments that handles TOML array format.
fn deserialize_block_comments<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<Vec<String>> = Vec::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|pair| {
            if pair.len() >= 2 {
                Some((pair[0].clone(), pair[1].clone()))
            } else {
                None
            }
        })
        .collect())
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
