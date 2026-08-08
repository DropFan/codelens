//! Code complexity analysis.

use crate::language::Language;

use super::stats::Complexity;

/// A function's location within a file, by line numbers.
///
/// Spans are heuristic: a function extends from its signature match to
/// the line before the next match (the last one runs to end of file).
/// Trailing items between functions get attributed to the preceding
/// function — good enough for change attribution, not for tooling that
/// needs exact boundaries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FunctionSpan {
    pub name: String,
    /// 1-based, inclusive.
    pub start_line: usize,
    /// 1-based, inclusive.
    pub end_line: usize,
}

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

        // Languages without any complexity signals configured (documents
        // and data formats like Markdown/HTML/JSON) get no complexity
        // metrics: bracket depth in a JSON file or a Markdown code block
        // is not a code-nesting signal.
        if patterns.function_re.is_none() && patterns.keywords_re.is_none() {
            return complexity;
        }

        // Count functions
        if let Some(ref re) = patterns.function_re {
            complexity.functions = re.find_iter(content).count();
        }

        // Count complexity keywords (single alternation regex, one pass)
        let keyword_offsets: Vec<usize> = patterns
            .keywords_re
            .as_ref()
            .map(|re| re.find_iter(content).map(|m| m.start()).collect())
            .unwrap_or_default();
        complexity.cyclomatic = keyword_offsets.len();

        // Base complexity is 1 per function
        complexity.cyclomatic += complexity.functions;

        // One bracket scan yields both the max nesting depth and the
        // nesting-weighted (cognitive) keyword cost.
        let (max_depth, cognitive) =
            self.scan_depth(content, &lang.line_comments, &keyword_offsets);
        complexity.max_depth = max_depth;
        complexity.cognitive = cognitive;

        // Calculate average lines per function
        if complexity.functions > 0 {
            let total_lines = content.lines().count();
            complexity.avg_func_lines = total_lines as f64 / complexity.functions as f64;
        }

        complexity
    }

    /// Locate function spans using the language's function pattern.
    /// Returns an empty list for languages without one.
    pub fn function_spans(&self, content: &str, lang: &Language) -> Vec<FunctionSpan> {
        let patterns = lang.complexity_patterns();
        let Some(re) = &patterns.function_re else {
            return Vec::new();
        };

        let matches: Vec<(usize, String)> = re
            .find_iter(content)
            .map(|m| {
                // Patterns like `(?m)^\s*fn ...` swallow preceding blank
                // lines into the match; anchor the span at the signature
                // itself, not at the leading whitespace.
                let lead_ws = m.as_str().len() - m.as_str().trim_start().len();
                (m.start() + lead_ws, trailing_identifier(m.as_str()))
            })
            .collect();
        if matches.is_empty() {
            return Vec::new();
        }

        // Byte offset of each line start, for offset → line translation.
        let mut line_starts = vec![0usize];
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset);
        let total_lines = content.lines().count().max(1);

        matches
            .iter()
            .enumerate()
            .map(|(i, (offset, name))| {
                let start_line = line_of(*offset);
                let end_line = if i + 1 < matches.len() {
                    line_of(matches[i + 1].0).saturating_sub(1).max(start_line)
                } else {
                    total_lines
                };
                FunctionSpan {
                    name: name.clone(),
                    start_line,
                    end_line,
                }
            })
            .collect()
    }

    /// One pass over the content computing (max nesting depth, cognitive
    /// complexity). Depth comes from bracket pairs, ignoring brackets
    /// inside string literals, char literals, and line comments —
    /// otherwise text like ANSI codes (`\x1b[1;32m`) in string constants
    /// inflates the depth without bound.
    ///
    /// Cognitive complexity: each control-flow keyword (already located
    /// by the caller, offsets ascending) costs its bracket depth at that
    /// point (min 1), so `if` nested three levels deep costs more than
    /// `if` at the top of a function.
    fn scan_depth(
        &self,
        content: &str,
        line_comments: &[String],
        keyword_offsets: &[usize],
    ) -> (usize, usize) {
        let bytes = content.as_bytes();
        let mut max_depth: usize = 0;
        let mut current_depth: usize = 0;
        let mut cognitive: usize = 0;
        let mut next_kw = 0;
        let mut i = 0;

        while i < bytes.len() {
            // Charge keywords we've reached (or jumped past when skipping
            // strings/comments) at the current depth.
            while next_kw < keyword_offsets.len() && keyword_offsets[next_kw] <= i {
                cognitive += current_depth.max(1);
                next_kw += 1;
            }

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
        // Keywords sitting past the final byte position (e.g. inside a
        // trailing string) still need charging.
        while next_kw < keyword_offsets.len() {
            cognitive += current_depth.max(1);
            next_kw += 1;
        }

        (max_depth, cognitive)
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Last identifier in a function signature match:
/// "pub async fn parse_log" → "parse_log".
fn trailing_identifier(matched: &str) -> String {
    matched
        .trim()
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|s| !s.is_empty())
        .unwrap_or("?")
        .to_string()
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
    fn test_no_complexity_for_document_languages() {
        let analyzer = ComplexityAnalyzer::new();
        // Markdown-like language: no function pattern, no keywords
        let lang = Language {
            name: "Markdown".to_string(),
            extensions: vec![".md".to_string()],
            ..Default::default()
        };

        let content = "# Title\n```json\n{\"a\":{\"b\":{\"c\":{\"d\":1}}}}\n```\n";
        let complexity = analyzer.analyze(content, &lang);
        assert_eq!(
            complexity.max_depth, 0,
            "documents must not report nesting depth"
        );
        assert_eq!(complexity.cyclomatic, 0);
    }

    #[test]
    fn test_max_depth_ignores_brackets_in_strings_and_comments() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        // Unbalanced brackets inside strings, char literals, and comments
        // must not count toward nesting depth. Built at runtime so this
        // source file itself contains no unbalanced bracket runs.
        let opens = "[".repeat(5);
        let parens = "(".repeat(5);
        let content = format!(
            "fn main() {{\n    let ansi = \"\\x1b[1;32m{opens}\";\n    let c = '[';\n    // brackets in a comment: {opens} {parens}\n    println!(\"{parens}\");\n}}\n",
        );
        let complexity = analyzer.analyze(&content, &lang);
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
    fn test_cognitive_weights_nesting() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        // Flat: two `if` at function-body depth.
        let flat = "fn f() { if a() { b(); } if c() { d(); } }\n";
        // Nested: the second `if` sits inside the first.
        let nested = "fn f() { if a() { if c() { d(); } } }\n";

        let flat_c = analyzer.analyze(flat, &lang);
        let nested_c = analyzer.analyze(nested, &lang);
        assert_eq!(
            flat_c.cyclomatic, nested_c.cyclomatic,
            "cyclomatic can't tell these apart"
        );
        assert!(
            nested_c.cognitive > flat_c.cognitive,
            "cognitive must punish nesting: flat {} vs nested {}",
            flat_c.cognitive,
            nested_c.cognitive
        );
    }

    #[test]
    fn test_cognitive_zero_for_documents() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = Language {
            name: "Markdown".to_string(),
            ..Default::default()
        };
        let c = analyzer.analyze("# if else while\n", &lang);
        assert_eq!(c.cognitive, 0);
    }

    #[test]
    fn test_function_spans() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = "\
fn first() {
    body();
}

pub fn second() {
    more();
}
";
        let spans = analyzer.function_spans(content, &lang);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].name, "first");
        assert_eq!(spans[0].start_line, 1);
        assert_eq!(spans[0].end_line, 4, "first span ends before second's line");
        assert_eq!(spans[1].name, "second");
        assert_eq!(spans[1].start_line, 5);
        assert_eq!(spans[1].end_line, 7, "last span runs to end of file");
    }

    #[test]
    fn test_function_spans_no_pattern() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = Language {
            name: "Markdown".to_string(),
            ..Default::default()
        };
        assert!(analyzer.function_spans("# hi\n", &lang).is_empty());
    }

    #[test]
    fn test_trailing_identifier() {
        assert_eq!(trailing_identifier("pub async fn parse_log"), "parse_log");
        assert_eq!(trailing_identifier("  def foo"), "foo");
        assert_eq!(trailing_identifier("function bar ("), "bar");
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
