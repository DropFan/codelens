//! Single file analyzer.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::error::Result;
use crate::language::{Language, LanguageRegistry};

use super::complexity::ComplexityAnalyzer;
use super::stats::{FileStats, LineStats};

/// Analyzes individual source files.
pub struct FileAnalyzer {
    registry: Arc<LanguageRegistry>,
    complexity_analyzer: ComplexityAnalyzer,
    min_lines: Option<usize>,
    max_lines: Option<usize>,
}

impl FileAnalyzer {
    /// Create a new file analyzer.
    pub fn new(registry: Arc<LanguageRegistry>, config: &Config) -> Self {
        Self {
            registry,
            complexity_analyzer: ComplexityAnalyzer::new(),
            min_lines: config.filter.min_lines,
            max_lines: config.filter.max_lines,
        }
    }

    /// Analyze a single file.
    ///
    /// Returns `None` if the file's language is not recognized.
    pub fn analyze(&self, path: &Path) -> Result<Option<FileStats>> {
        // Detect language
        let language = match self.registry.detect(path) {
            Some(lang) => lang,
            None => return Ok(None),
        };

        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                // Try reading as lossy UTF-8
                match fs::read(path) {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                    Err(e) => {
                        return Err(crate::error::Error::FileRead {
                            path: path.to_path_buf(),
                            source: e,
                        })
                    }
                }
            }
        };

        // Count lines
        let lines = self.count_lines(&content, &language);

        // Apply line filters
        if let Some(min) = self.min_lines {
            if lines.total < min {
                return Ok(None);
            }
        }
        if let Some(max) = self.max_lines {
            if lines.total > max {
                return Ok(None);
            }
        }

        // Get file size
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        // Analyze complexity
        let complexity = self.complexity_analyzer.analyze(&content, &language);

        Ok(Some(FileStats {
            path: path.to_path_buf(),
            language: language.name.clone(),
            lines,
            size,
            complexity,
        }))
    }

    /// Count lines in file content.
    fn count_lines(&self, content: &str, lang: &Language) -> LineStats {
        let mut stats = LineStats::default();
        let mut in_block_comment = false;
        let mut block_comment_end = "";

        for line in content.lines() {
            stats.total += 1;
            let trimmed = line.trim();

            // Empty line
            if trimmed.is_empty() {
                stats.blank += 1;
                continue;
            }

            // Inside block comment
            if in_block_comment {
                stats.comment += 1;
                if let Some(pos) = trimmed.find(block_comment_end) {
                    // Check if there's code after the comment end
                    let after = trimmed[pos + block_comment_end.len()..].trim();
                    if !after.is_empty() && !self.starts_with_comment(after, lang) {
                        // Line has code after comment - count as code too
                        // But we already counted as comment, so adjust
                        stats.comment -= 1;
                        stats.code += 1;
                    }
                    in_block_comment = false;
                }
                continue;
            }

            // Check for block comment start
            let mut found_block_start = false;
            for (start, end) in &lang.block_comments {
                if let Some(start_pos) = trimmed.find(start.as_str()) {
                    // Check if it's inside a string (simplified check)
                    let before = &trimmed[..start_pos];
                    if self.is_in_string(before, lang) {
                        continue;
                    }

                    found_block_start = true;
                    let after_start = &trimmed[start_pos + start.len()..];

                    if let Some(end_pos) = after_start.find(end.as_str()) {
                        // Single-line block comment
                        let after_end = after_start[end_pos + end.len()..].trim();
                        if before.trim().is_empty() && after_end.is_empty() {
                            stats.comment += 1;
                        } else {
                            // Mixed line - count as code
                            stats.code += 1;
                        }
                    } else {
                        // Multi-line block comment starts
                        in_block_comment = true;
                        block_comment_end = end;
                        if before.trim().is_empty() {
                            stats.comment += 1;
                        } else {
                            // Code before comment start
                            stats.code += 1;
                        }
                    }
                    break;
                }
            }

            if found_block_start {
                continue;
            }

            // Check for line comment
            let is_line_comment = lang
                .line_comments
                .iter()
                .any(|prefix| trimmed.starts_with(prefix.as_str()));

            if is_line_comment {
                stats.comment += 1;
            } else {
                stats.code += 1;
            }
        }

        stats
    }

    /// Check if a string position is likely inside a string literal.
    fn is_in_string(&self, text: &str, _lang: &Language) -> bool {
        // Simplified check: count unescaped quotes
        let mut in_string = false;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '"' | '\'' => {
                    in_string = !in_string;
                }
                '\\' => {
                    // Skip escaped character
                    chars.next();
                }
                _ => {}
            }
        }

        in_string
    }

    /// Check if text starts with a comment.
    fn starts_with_comment(&self, text: &str, lang: &Language) -> bool {
        lang.line_comments
            .iter()
            .any(|prefix| text.starts_with(prefix.as_str()))
            || lang
                .block_comments
                .iter()
                .any(|(start, _)| text.starts_with(start.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rust_lang() -> Language {
        Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            filenames: vec![],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            string_delimiters: vec![],
            function_pattern: None,
            complexity_keywords: vec![],
            nested_comments: true,
        }
    }

    #[test]
    fn test_count_lines_basic() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = r#"
fn main() {
    println!("hello");
}
"#;
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.blank, 2);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_count_lines_with_comments() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = r#"// This is a comment
fn main() {
    /* block comment */
    println!("hello");
}
"#;
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 6);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.comment, 2);
        assert_eq!(stats.blank, 1);
    }

    #[test]
    fn test_count_lines_multiline_comment() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = r#"/*
 * Multi-line
 * comment
 */
fn main() {}
"#;
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 6);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 4);
        assert_eq!(stats.blank, 1);
    }
}
