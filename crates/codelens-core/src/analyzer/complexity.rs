//! Code complexity analysis.

use crate::language::Language;
use regex::Regex;

use super::stats::Complexity;
use super::trie::{should_process, TokenType};

/// A function's location within a file, by line numbers.
///
/// Brace-delimited functions end at their matching closing brace. Languages
/// without brace-delimited bodies fall back to the line before the next
/// function match (the last one runs to end of file).
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

        // Regexes and bracket scans must only see source code. Keeping byte
        // positions and newlines intact lets offsets and line numbers still
        // refer to the original file.
        let code = mask_non_code(content, lang);

        let function_spans = patterns
            .function_re
            .as_ref()
            .map(|re| function_spans_in(&code, re, lang))
            .unwrap_or_default();
        complexity.functions = function_spans.len();

        // Count complexity keywords (single alternation regex, one pass)
        let keyword_offsets: Vec<usize> = patterns
            .keywords_re
            .as_ref()
            .map(|re| re.find_iter(&code).map(|m| m.start()).collect())
            .unwrap_or_default();
        complexity.cyclomatic = keyword_offsets.len();

        // Base complexity is 1 per function
        complexity.cyclomatic += complexity.functions;

        // One bracket scan yields both the max nesting depth and the
        // nesting-weighted (cognitive) keyword cost.
        let (max_depth, cognitive) = self.scan_depth(&code, &keyword_offsets);
        complexity.max_depth = max_depth;
        complexity.cognitive = cognitive;

        if !function_spans.is_empty() {
            let function_lines: usize = function_spans
                .iter()
                .map(|span| span.end_line - span.start_line + 1)
                .sum();
            complexity.avg_func_lines = function_lines as f64 / function_spans.len() as f64;
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
        let code = mask_non_code(content, lang);
        function_spans_in(&code, re, lang)
    }

    /// One pass over the content computing (max nesting depth, cognitive
    /// complexity). Depth comes from bracket pairs. The caller supplies
    /// source with comments and strings already masked.
    ///
    /// Cognitive complexity: each control-flow keyword (already located
    /// by the caller, offsets ascending) costs its bracket depth at that
    /// point (min 1), so `if` nested three levels deep costs more than
    /// `if` at the top of a function.
    fn scan_depth(&self, content: &str, keyword_offsets: &[usize]) -> (usize, usize) {
        let bytes = content.as_bytes();
        let mut max_depth: usize = 0;
        let mut current_depth: usize = 0;
        let mut cognitive: usize = 0;
        let mut next_kw = 0;
        let mut i = 0;

        while i < bytes.len() {
            // Charge keywords reached at the current bracket depth.
            while next_kw < keyword_offsets.len() && keyword_offsets[next_kw] <= i {
                cognitive += current_depth.max(1);
                next_kw += 1;
            }

            match bytes[i] {
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
        // Retain a defensive tail pass if a future matcher reports an offset
        // at the end of the source.
        while next_kw < keyword_offsets.len() {
            cognitive += current_depth.max(1);
            next_kw += 1;
        }

        (max_depth, cognitive)
    }
}

fn function_spans_in(content: &str, re: &Regex, lang: &Language) -> Vec<FunctionSpan> {
    let matches: Vec<(usize, usize, String)> = re
        .find_iter(content)
        .map(|m| {
            // Patterns like `(?m)^\s*fn ...` swallow preceding blank lines;
            // anchor the span at the signature rather than that whitespace.
            let lead_ws = m.as_str().len() - m.as_str().trim_start().len();
            (
                m.start() + lead_ws,
                m.end(),
                trailing_identifier(m.as_str()),
            )
        })
        .collect();
    if matches.is_empty() {
        return Vec::new();
    }

    let mut line_starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    let line_of = |offset: usize| line_starts.partition_point(|&start| start <= offset);
    let total_lines = content.lines().count().max(1);

    matches
        .iter()
        .enumerate()
        .map(|(i, (start_offset, match_end, name))| {
            let start_line = line_of(*start_offset);
            let next_start = matches
                .get(i + 1)
                .map(|next| next.0)
                .unwrap_or(content.len());
            let fallback_end = line_of(next_start)
                .saturating_sub(1)
                .max(start_line)
                .min(total_lines);
            let end_line = brace_body_end(
                content.as_bytes(),
                *start_offset,
                *match_end,
                next_start,
                uses_curly_function_bodies(&lang.name),
            )
            .map(line_of)
            .unwrap_or(fallback_end);
            FunctionSpan {
                name: name.clone(),
                start_line,
                end_line,
            }
        })
        .collect()
}

fn uses_curly_function_bodies(language: &str) -> bool {
    matches!(
        language,
        "Rust"
            | "C"
            | "C++"
            | "Zig"
            | "Go"
            | "Java"
            | "Kotlin"
            | "Groovy"
            | "C#"
            | "PHP"
            | "Bash"
            | "Zsh"
            | "Fish"
            | "PowerShell"
            | "JavaScript"
            | "TypeScript"
            | "Swift"
            | "Objective-C"
            | "Dart"
            | "R"
            | "Solidity"
    )
}

/// Return the byte offset of a brace-delimited function's closing brace, or
/// of a declaration's semicolon. For unknown/custom languages, only accept an
/// opening brace on the signature line to avoid mistaking a body expression
/// (for example, a Python dictionary) for the function body.
fn brace_body_end(
    content: &[u8],
    match_start: usize,
    match_end: usize,
    search_end: usize,
    allow_multiline_signature: bool,
) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut i = match_start;

    while i < search_end {
        match content[i] {
            b'\n' if !allow_multiline_signature && i >= match_end => return None,
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'{' if paren_depth == 0 && bracket_depth == 0 => {
                return matching_brace_end(content, i);
            }
            b';' if i >= match_end && paren_depth == 0 && bracket_depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn matching_brace_end(content: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in content[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Replace comment and string bytes with spaces while preserving byte offsets
/// and line endings. The language token trie keeps this scanner aligned with
/// the line counter's delimiter rules without requiring a full parser.
fn mask_non_code(content: &str, lang: &Language) -> String {
    let source = content.as_bytes();
    let mut masked = source.to_vec();
    let (trie, process_mask) = lang.tokens();
    let mut i = 0;
    let mut at_line_start = true;

    while i < source.len() {
        if source[i] == b'\n' {
            at_line_start = true;
            i += 1;
            continue;
        }

        let token_at_line_start = at_line_start;
        at_line_start = false;
        if !should_process(source[i], *process_mask) {
            i += 1;
            continue;
        }
        let Some(token) = trie
            .match_at(source, i)
            .filter(|token| !token.line_start_only || token_at_line_start)
        else {
            i += 1;
            continue;
        };

        let start = i;
        match token.token_type {
            TokenType::LineComment => {
                while i < source.len() && source[i] != b'\n' {
                    i += 1;
                }
            }
            TokenType::BlockCommentStart => {
                let open = source[start..start + token.advance].to_vec();
                let close = token.close.unwrap_or_default();
                i += token.advance;
                let mut nested_depth = 0usize;
                let mut comment_line_start = false;

                while i < source.len() {
                    if source[i] == b'\n' {
                        comment_line_start = true;
                        i += 1;
                        continue;
                    }
                    let anchored = !token.line_start_only || comment_line_start;
                    comment_line_start = false;
                    if anchored && bytes_match_at(source, i, &close) {
                        i += close.len();
                        if nested_depth == 0 {
                            break;
                        }
                        nested_depth -= 1;
                        continue;
                    }
                    if token.nested && anchored && bytes_match_at(source, i, &open) {
                        nested_depth += 1;
                        i += open.len();
                        continue;
                    }
                    i += 1;
                }
            }
            TokenType::StringDelimiter | TokenType::DocStringDelimiter => {
                let close = token.close.unwrap_or_default();
                i += token.advance;
                while i < source.len() {
                    if source[i] == b'\n' && !token.multiline {
                        break;
                    }
                    if token.escape == Some(source[i]) {
                        i = (i + 2).min(source.len());
                        continue;
                    }
                    if bytes_match_at(source, i, &close) {
                        i += close.len();
                        break;
                    }
                    i += 1;
                }
            }
        }
        mask_range(&mut masked, start, i);
    }

    String::from_utf8(masked).expect("masking valid UTF-8 must preserve UTF-8")
}

fn mask_range(content: &mut [u8], start: usize, end: usize) {
    for byte in &mut content[start..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn bytes_match_at(content: &[u8], pos: usize, pattern: &[u8]) -> bool {
    !pattern.is_empty()
        && pos + pattern.len() <= content.len()
        && &content[pos..pos + pattern.len()] == pattern
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
                "?".to_string(),
                "&&".to_string(),
                "||".to_string(),
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
    fn test_complexity_ignores_comments_strings_and_fake_functions() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = r#"
fn real() {
    let text = "if for match fn fake() { [[[(";
    // while else fn commented_out() { ((
    /* if /* match && */ || fn also_fake() { [[ */
}
"#;

        let complexity = analyzer.analyze(content, &lang);
        assert_eq!(complexity.functions, 1);
        assert_eq!(complexity.cyclomatic, 1);
        assert_eq!(complexity.cognitive, 0);
        assert_eq!(complexity.max_depth, 1);
    }

    #[test]
    fn test_complexity_counts_symbolic_operators_once() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = "fn f() { if a && b || c { value()?; } }\n";

        let complexity = analyzer.analyze(content, &lang);
        // 1 function + if + && + || + ?
        assert_eq!(complexity.cyclomatic, 5);
    }

    #[test]
    fn test_average_function_length_excludes_file_preamble() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = "// module documentation\nuse std::fmt;\n\nfn only() {}\n";

        let complexity = analyzer.analyze(content, &lang);
        assert_eq!(complexity.functions, 1);
        assert_eq!(complexity.avg_func_lines, 1.0);
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
        assert_eq!(spans[0].end_line, 3, "first span ends at its closing brace");
        assert_eq!(spans[1].name, "second");
        assert_eq!(spans[1].start_line, 5);
        assert_eq!(spans[1].end_line, 7, "last span runs to end of file");
    }

    #[test]
    fn test_function_spans_stop_at_declaration_semicolon() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = "fn declared();\nfn implemented() {\n    work();\n}\n";

        let spans = analyzer.function_spans(content, &lang);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].end_line, 1);
        assert_eq!(spans[1].end_line, 4);
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
