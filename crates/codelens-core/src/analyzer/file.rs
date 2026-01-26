//! Single file analyzer.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::error::Result;
use crate::language::{Language, LanguageRegistry};

use super::complexity::ComplexityAnalyzer;
use super::stats::{FileStats, LineStats};

/// Represents a string delimiter for multiline string detection.
#[derive(Debug, Clone)]
struct StringDelimiter {
    /// The closing delimiter pattern
    end_pattern: String,
    /// Whether this is a raw string (no escape processing)
    is_raw: bool,
    /// Whether this is a docstring (Python) - should be counted as comment
    is_docstring: bool,
}

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
        let mut in_multiline_string = false;
        let mut string_delimiter: Option<StringDelimiter> = None;

        for line in content.lines() {
            stats.total += 1;
            let trimmed = line.trim();

            // Empty line
            if trimmed.is_empty() {
                stats.blank += 1;
                continue;
            }

            // Inside multiline string
            if in_multiline_string {
                if let Some(ref delim) = string_delimiter {
                    // Docstrings count as comments, regular strings as code
                    if delim.is_docstring {
                        stats.comment += 1;
                    } else {
                        stats.code += 1;
                    }
                    if self.line_ends_string(line, delim) {
                        in_multiline_string = false;
                        string_delimiter = None;
                    }
                }
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

            // Check if line starts a multiline string
            // starts_multiline_string only returns Some if the string is NOT closed on the same line
            if let Some(delim) = self.starts_multiline_string(line, lang) {
                // Docstrings count as comments, regular strings as code
                if delim.is_docstring {
                    stats.comment += 1;
                } else {
                    stats.code += 1;
                }
                in_multiline_string = true;
                string_delimiter = Some(delim);
                continue;
            }

            // Check for single-line Python docstring ("""...""" on one line)
            if lang.name == "Python" {
                if let Some(is_docstring) = self.is_single_line_docstring(trimmed) {
                    if is_docstring {
                        stats.comment += 1;
                    } else {
                        stats.code += 1;
                    }
                    continue;
                }
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

    /// Check if a line starts a multiline string literal.
    /// Returns the delimiter info if a multiline string starts on this line.
    fn starts_multiline_string(&self, line: &str, lang: &Language) -> Option<StringDelimiter> {
        // Check for Rust raw strings: r#"..."# or r##"..."##
        if lang.name == "Rust" {
            if let Some(delim) = self.detect_rust_raw_string_start(line) {
                return Some(delim);
            }
        }

        // Check for Python triple-quoted strings (including docstrings)
        if lang.name == "Python" {
            for pattern in &["\"\"\"", "'''"] {
                if let Some(pos) = line.find(pattern) {
                    let before = &line[..pos];
                    if !self.is_in_string(before, lang) {
                        let after = &line[pos + 3..];
                        // Check if it closes on the same line
                        if after.find(pattern).is_none() {
                            // Docstring: no assignment before the triple quotes
                            let is_docstring = !before.contains('=');
                            return Some(StringDelimiter {
                                end_pattern: pattern.to_string(),
                                is_raw: false,
                                is_docstring,
                            });
                        }
                    }
                }
            }
        }

        // Check for regular multiline strings (string not closed on same line)
        let mut in_string = false;
        let mut string_char = '"';
        let mut escape_next = false;

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];

            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }

            if c == '\\' && in_string {
                escape_next = true;
                i += 1;
                continue;
            }

            if (c == '"' || c == '\'') && !in_string {
                // Calculate byte position for string slice
                let byte_pos: usize = chars[..i].iter().map(|ch| ch.len_utf8()).sum();
                let before = &line[..byte_pos];
                if !self.is_in_string(before, lang) {
                    in_string = true;
                    string_char = c;
                }
            } else if c == string_char && in_string {
                in_string = false;
            }

            i += 1;
        }

        if in_string {
            return Some(StringDelimiter {
                end_pattern: string_char.to_string(),
                is_raw: false,
                is_docstring: false,
            });
        }

        None
    }

    /// Detect Rust raw string start (r#"..."# or r##"..."##, etc.)
    fn detect_rust_raw_string_start(&self, line: &str) -> Option<StringDelimiter> {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Look for 'r' followed by optional '#' and '"'
            if bytes[i] == b'r' && i + 1 < len {
                let start = i;
                i += 1;

                // Count the number of '#' after 'r'
                let mut hash_count = 0;
                while i < len && bytes[i] == b'#' {
                    hash_count += 1;
                    i += 1;
                }

                // Check for opening '"'
                if i < len && bytes[i] == b'"' {
                    // Check that 'r' is not part of an identifier
                    if start == 0 || !bytes[start - 1].is_ascii_alphanumeric() {
                        // Build the closing pattern: "# repeated hash_count times
                        let end_pattern = format!("\"{}", "#".repeat(hash_count));

                        // Check if it closes on the same line
                        let after_quote = &line[i + 1..];
                        if after_quote.find(&end_pattern).is_none() {
                            return Some(StringDelimiter {
                                end_pattern,
                                is_raw: true,
                                is_docstring: false,
                            });
                        }
                    }
                }
            }
            i += 1;
        }

        None
    }

    /// Check if a line ends the current multiline string.
    fn line_ends_string(&self, line: &str, delim: &StringDelimiter) -> bool {
        if delim.is_raw {
            // For raw strings, just look for the closing pattern
            line.contains(&delim.end_pattern)
        } else {
            // For regular strings, need to handle escapes
            let mut chars = line.chars().peekable();
            let target: Vec<char> = delim.end_pattern.chars().collect();

            while let Some(c) = chars.next() {
                if c == '\\' {
                    // Skip escaped character
                    chars.next();
                    continue;
                }

                if !target.is_empty() && c == target[0] {
                    // Check if this matches the closing pattern
                    let mut matched = true;
                    for expected in target.iter().skip(1) {
                        if chars.next() != Some(*expected) {
                            matched = false;
                            break;
                        }
                    }
                    if matched {
                        return true;
                    }
                }
            }
            false
        }
    }

    /// Check if a line is a single-line Python docstring.
    /// Returns Some(true) if it's a docstring, Some(false) if it's a regular string assignment,
    /// None if it doesn't contain a complete triple-quoted string.
    fn is_single_line_docstring(&self, trimmed: &str) -> Option<bool> {
        for pattern in &["\"\"\"", "'''"] {
            if let Some(start_pos) = trimmed.find(pattern) {
                let after_start = &trimmed[start_pos + 3..];
                // Check if it closes on the same line
                if let Some(end_pos) = after_start.find(pattern) {
                    // Make sure there's nothing significant after the closing quotes
                    let after_end = after_start[end_pos + 3..].trim();
                    if after_end.is_empty() || after_end.starts_with('#') {
                        // It's a complete triple-quoted string on one line
                        let before = &trimmed[..start_pos];
                        // Docstring: no assignment before the triple quotes
                        return Some(!before.contains('='));
                    }
                }
            }
        }
        None
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

        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.blank, 0);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_count_lines_with_comments() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = "// This is a comment\nfn main() {\n    /* block comment */\n    println!(\"hello\");\n}\n";
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.comment, 2);
        assert_eq!(stats.blank, 0);
    }

    #[test]
    fn test_count_lines_multiline_comment() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = "/*\n * Multi-line\n * comment\n */\nfn main() {}\n";
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 4);
        assert_eq!(stats.blank, 0);
    }

    #[test]
    fn test_count_lines_multiline_string() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        // Multiline string with content that looks like a comment
        let content = "let s = \"hello\n// not a comment\nworld\";\n";
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.code, 3, "All lines should be code (inside string)");
        assert_eq!(stats.comment, 0, "No comments - // is inside string");
        assert_eq!(stats.blank, 0);
    }

    #[test]
    fn test_count_lines_raw_string() {
        let lang = make_rust_lang();
        let registry = Arc::new(LanguageRegistry::empty());
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        // Raw string with content that looks like a comment
        let content = "let s = r#\"hello\n// not a comment\n/* also not */\nworld\"#;\n";
        let stats = analyzer.count_lines(content, &lang);
        assert_eq!(stats.total, 4);
        assert_eq!(stats.code, 4, "All lines should be code (inside raw string)");
        assert_eq!(stats.comment, 0, "No comments - everything is inside raw string");
        assert_eq!(stats.blank, 0);
    }
}
