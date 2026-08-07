//! Code complexity analysis.

use crate::language::Language;

use super::stats::Complexity;

/// Analyzes code complexity metrics.
pub struct ComplexityAnalyzer;

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze complexity metrics for the given content.
    ///
    /// Uses precompiled regex patterns cached on the Language via OnceLock.
    pub fn analyze(&self, content: &str, lang: &Language) -> Complexity {
        let mut complexity = Complexity::default();
        let patterns = lang.complexity_patterns();

        // Count functions
        if let Some(ref re) = patterns.function_re {
            complexity.functions = re.find_iter(content).count();
        }

        // Count complexity keywords (single alternation regex, one pass)
        if let Some(ref re) = patterns.keywords_re {
            complexity.cyclomatic = re.find_iter(content).count();
        }

        // Base complexity is 1 per function
        complexity.cyclomatic += complexity.functions;

        // Calculate max nesting depth
        complexity.max_depth = self.calculate_max_depth(content, &lang.line_comments);

        // Calculate average lines per function
        if complexity.functions > 0 {
            let total_lines = content.lines().count();
            complexity.avg_func_lines = total_lines as f64 / complexity.functions as f64;
        }

        complexity
    }

    /// Calculate maximum nesting depth from bracket pairs, ignoring
    /// brackets inside string literals, char literals, and line comments —
    /// otherwise text like ANSI codes (`\x1b[1;32m`) in string constants
    /// inflates the depth without bound.
    fn calculate_max_depth(&self, content: &str, line_comments: &[String]) -> usize {
        let bytes = content.as_bytes();
        let mut max_depth: usize = 0;
        let mut current_depth: usize = 0;
        let mut i = 0;

        while i < bytes.len() {
            // Line comment: skip to end of line
            if line_comments
                .iter()
                .any(|c| bytes[i..].starts_with(c.as_bytes()))
            {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            match bytes[i] {
                // Double-quoted string: skip to the closing quote, honoring
                // backslash escapes. Scans across newlines so multi-line
                // string constants don't leak their brackets into the count.
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                // Char literal 'X' or '\X' — bounded lookahead so Rust
                // lifetimes ('a) are NOT treated as strings
                b'\'' => {
                    if i + 3 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 3] == b'\'' {
                        i += 3;
                    } else if i + 2 < bytes.len() && bytes[i + 2] == b'\'' {
                        i += 2;
                    }
                }
                b'{' | b'(' | b'[' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                b'}' | b')' | b']' => {
                    current_depth = current_depth.saturating_sub(1);
                }
                _ => {}
            }
            i += 1;
        }

        max_depth
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rust_lang() -> Language {
        Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            function_pattern: Some(r"(?m)^\s*(pub\s+)?(async\s+)?fn\s+\w+".to_string()),
            complexity_keywords: vec![
                "if".to_string(),
                "else".to_string(),
                "for".to_string(),
                "while".to_string(),
                "match".to_string(),
            ],
            nested_comments: true,
            ..Default::default()
        }
    }

    #[test]
    fn test_count_functions() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    println!("hello");
}

pub fn helper() {}

pub async fn async_fn() {}
"#;

        let complexity = analyzer.analyze(content, &lang);
        assert_eq!(complexity.functions, 3);
    }

    #[test]
    fn test_cyclomatic_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    if true {
        for i in 0..10 {
            if i > 5 {
                println!("{}", i);
            }
        }
    } else {
        while false {}
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);
        // 1 function + 2 if + 1 for + 1 else + 1 while = 6
        assert_eq!(complexity.cyclomatic, 6);
    }

    #[test]
    fn test_max_depth_ignores_brackets_in_strings_and_comments() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        // Unbalanced brackets inside strings, char literals, and comments
        // must not count toward nesting depth
        let content = r#"
fn main() {
    let ansi = "\x1b[1;32m[[[[[";
    let c = '[';
    // opening brackets in a comment: [[[ (((
    println!("(((((");
}
"#;
        let complexity = analyzer.analyze(content, &lang);
        assert!(
            complexity.max_depth <= 3,
            "string/comment brackets must be ignored, got depth {}",
            complexity.max_depth
        );
    }

    #[test]
    fn test_max_depth_lifetime_is_not_a_string() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        // A lifetime tick must not open a "string" that would swallow
        // the real brackets that follow it
        let content = "fn f<'a>(x: &'a str) { if true { g(x); } }\n";
        let complexity = analyzer.analyze(content, &lang);
        assert!(
            complexity.max_depth >= 2,
            "brackets after lifetimes must still count, got {}",
            complexity.max_depth
        );
    }

    #[test]
    fn test_max_depth() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    if true {
        for i in 0..10 {
            match i {
                _ => {}
            }
        }
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);
        assert!(complexity.max_depth >= 4);
    }
}
