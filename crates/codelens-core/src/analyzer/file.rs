//! Single file analyzer.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::error::Result;
use crate::language::LanguageRegistry;

use super::complexity::ComplexityAnalyzer;
use super::counter;
use super::stats::{FileStats, LineStats};

/// Maximum bytes to inspect for binary detection.
const BINARY_CHECK_LEN: usize = 10 * 1024;

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
    /// Returns `None` if the file's language is not recognized or the file is binary.
    pub fn analyze(&self, path: &Path) -> Result<Option<FileStats>> {
        let content = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Err(crate::error::Error::FileRead {
                    path: path.to_path_buf(),
                    source: e,
                })
            }
        };

        self.analyze_from_bytes(path, &content)
    }

    /// Analyze a file from pre-read bytes (for buffer reuse).
    ///
    /// Returns `None` if the file's language is not recognized or the file is binary.
    pub fn analyze_from_bytes(&self, path: &Path, content: &[u8]) -> Result<Option<FileStats>> {
        // Detect language
        let language = match self.registry.detect(path) {
            Some(lang) => lang,
            None => return Ok(None),
        };

        // Detect binary files: check first 10KB for null bytes
        let check_len = content.len().min(BINARY_CHECK_LEN);
        if content[..check_len].contains(&0) {
            return Ok(None);
        }

        // Count lines using byte-level state machine
        let (trie, mask) = language.tokens();
        let lines: LineStats = counter::count_stats(content, trie, *mask);

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

        // File size from content length
        let size = content.len() as u64;

        // Analyze complexity (needs string representation)
        let text = String::from_utf8_lossy(content);
        let complexity = self.complexity_analyzer.analyze(&text, &language);

        Ok(Some(FileStats {
            path: path.to_path_buf(),
            language: language.name.clone(),
            lines,
            size,
            complexity,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_rust_registry() -> Arc<LanguageRegistry> {
        let mut registry = LanguageRegistry::empty();
        registry
            .load_toml(
                r#"
                [rust]
                name = "Rust"
                extensions = [".rs"]
                line_comments = ["//"]
                block_comments = [["/*", "*/"]]
                nested_comments = true
            "#,
            )
            .unwrap();
        Arc::new(registry)
    }

    #[test]
    fn test_analyze_from_bytes_basic_rust() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = b"fn main() {\n    println!(\"hello\");\n}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap().unwrap();

        assert_eq!(result.lines.total, 3);
        assert_eq!(result.lines.code, 3);
        assert_eq!(result.lines.blank, 0);
        assert_eq!(result.lines.comment, 0);
        assert_eq!(result.language, "Rust");
        assert_eq!(result.size, content.len() as u64);
    }

    #[test]
    fn test_analyze_from_bytes_with_comments() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = b"// This is a comment\nfn main() {\n    println!(\"hello\");\n}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap().unwrap();

        assert_eq!(result.lines.total, 4);
        assert_eq!(result.lines.code, 3);
        assert_eq!(result.lines.comment, 1);
    }

    #[test]
    fn test_analyze_from_bytes_multiline_block_comment() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = b"/*\n * Multi-line\n * comment\n */\nfn main() {}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap().unwrap();

        assert_eq!(result.lines.total, 5);
        assert_eq!(result.lines.code, 1);
        assert_eq!(result.lines.comment, 4);
    }

    #[test]
    fn test_analyze_from_bytes_returns_none_for_unknown_language() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = b"some content";
        let path = Path::new("test.xyz");
        let result = analyzer.analyze_from_bytes(path, content).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_analyze_from_bytes_detects_binary() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let mut content = b"fn main() {}\n".to_vec();
        content.push(0); // null byte makes it binary
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, &content).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_analyze_from_bytes_complexity() {
        let mut registry = LanguageRegistry::empty();
        registry
            .load_toml(
                r#"
                [rust]
                name = "Rust"
                extensions = [".rs"]
                line_comments = ["//"]
                block_comments = [["/*", "*/"]]
                nested_comments = true
                function_pattern = '(?m)^\s*(pub\s+)?(async\s+)?fn\s+\w+'
                complexity_keywords = ["if", "for"]
            "#,
            )
            .unwrap();
        let registry = Arc::new(registry);
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let content = b"fn main() {\n    if true {\n        for i in 0..10 {}\n    }\n}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap().unwrap();

        assert_eq!(result.complexity.functions, 1);
        assert!(result.complexity.cyclomatic >= 3); // 1 fn + 1 if + 1 for
    }

    #[test]
    fn test_analyze_from_bytes_line_filter_min() {
        let registry = make_rust_registry();
        let mut config = Config::default();
        config.filter.min_lines = Some(10);
        let analyzer = FileAnalyzer::new(registry, &config);

        let content = b"fn main() {}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap();

        assert!(
            result.is_none(),
            "File with 1 line should be filtered by min_lines=10"
        );
    }

    #[test]
    fn test_analyze_from_bytes_line_filter_max() {
        let registry = make_rust_registry();
        let mut config = Config::default();
        config.filter.max_lines = Some(1);
        let analyzer = FileAnalyzer::new(registry, &config);

        let content = b"fn main() {\n    println!(\"hello\");\n}\n";
        let path = Path::new("test.rs");
        let result = analyzer.analyze_from_bytes(path, content).unwrap();

        assert!(
            result.is_none(),
            "File with 3 lines should be filtered by max_lines=1"
        );
    }

    #[test]
    fn test_analyze_reads_file_from_disk() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let mut tmp = NamedTempFile::with_suffix(".rs").unwrap();
        write!(tmp, "fn main() {{}}\n").unwrap();

        let result = analyzer.analyze(tmp.path()).unwrap().unwrap();
        assert_eq!(result.lines.total, 1);
        assert_eq!(result.lines.code, 1);
        assert_eq!(result.size, 13); // "fn main() {}\n" is 13 bytes
    }

    #[test]
    fn test_analyze_delegates_to_analyze_from_bytes() {
        let registry = make_rust_registry();
        let analyzer = FileAnalyzer::new(registry, &Config::default());

        let mut tmp = NamedTempFile::with_suffix(".rs").unwrap();
        let content = b"// comment\nfn main() {}\n";
        tmp.write_all(content).unwrap();

        let from_disk = analyzer.analyze(tmp.path()).unwrap().unwrap();
        let from_bytes = analyzer
            .analyze_from_bytes(tmp.path(), content)
            .unwrap()
            .unwrap();

        assert_eq!(from_disk.lines, from_bytes.lines);
        assert_eq!(from_disk.size, from_bytes.size);
        assert_eq!(from_disk.language, from_bytes.language);
    }
}
