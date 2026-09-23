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

#[derive(Clone, Copy)]
struct KeywordOccurrence {
    offset: usize,
    is_control_flow: bool,
}

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
        complexity.functions_measured = patterns.function_re.is_some();
        complexity.control_flow_measured = patterns.keywords_re.is_some();
        complexity.legacy_functions = Some(0);
        complexity.legacy_cyclomatic = Some(0);
        complexity.legacy_max_depth = Some(0);
        complexity.legacy_avg_func_lines = Some(0.0);

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
        let legacy_function_spans = lang
            .legacy_function_re()
            .map(|re| legacy_function_spans_in(&code, re, lang))
            .unwrap_or_default();

        // Count complexity keywords (single alternation regex, one pass)
        let keyword_occurrences: Vec<KeywordOccurrence> = patterns
            .keywords_re
            .as_ref()
            .map(|re| {
                re.find_iter(&code)
                    .map(|matched| KeywordOccurrence {
                        offset: matched.start(),
                        is_control_flow: is_control_flow_keyword(matched.as_str()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        complexity.cyclomatic = keyword_occurrences.len();

        // Base complexity is 1 per function
        complexity.cyclomatic += complexity.functions;
        complexity.legacy_functions = Some(legacy_function_spans.len());
        complexity.legacy_cyclomatic =
            Some(keyword_occurrences.len() + legacy_function_spans.len());
        complexity.legacy_max_depth = Some(scan_legacy_bracket_depth(&code));

        let (max_depth, cognitive) = self.scan_depth(
            &code,
            &keyword_occurrences,
            &function_spans,
            uses_brace_control_flow(&lang.name),
        );
        complexity.max_depth = max_depth;
        complexity.cognitive = cognitive;

        if !function_spans.is_empty() {
            let function_lines: usize = function_spans
                .iter()
                .map(|span| span.end_line - span.start_line + 1)
                .sum();
            complexity.avg_func_lines = function_lines as f64 / function_spans.len() as f64;
        }
        if !legacy_function_spans.is_empty() {
            let function_lines: usize = legacy_function_spans
                .iter()
                .map(|span| span.end_line - span.start_line + 1)
                .sum();
            complexity.legacy_avg_func_lines =
                Some(function_lines as f64 / legacy_function_spans.len() as f64);
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

    /// Compute control-flow nesting and nesting-weighted keyword cost.
    /// Braced languages count only blocks opened by control-flow keywords;
    /// indentation languages rank the indentation of control-flow lines.
    fn scan_depth(
        &self,
        content: &str,
        keywords: &[KeywordOccurrence],
        function_spans: &[FunctionSpan],
        uses_braces: bool,
    ) -> (usize, usize) {
        if uses_braces {
            return scan_braced_depth(content, keywords);
        }
        scan_indented_depth(content, keywords, function_spans)
    }
}

fn scan_braced_depth(content: &str, keywords: &[KeywordOccurrence]) -> (usize, usize) {
    let bytes = content.as_bytes();
    let mut max_depth: usize = 0;
    let mut control_depth: usize = 0;
    let mut cognitive: usize = 0;
    let mut next_kw = 0;
    let mut pending_controls: Vec<(usize, usize)> = Vec::new();
    let mut brace_stack = Vec::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        while next_kw < keywords.len() && keywords[next_kw].offset <= i {
            let keyword = keywords[next_kw];
            let depth = if keyword.is_control_flow || !pending_controls.is_empty() {
                control_depth + 1
            } else {
                control_depth.max(1)
            };
            cognitive += depth;
            if keyword.is_control_flow {
                max_depth = max_depth.max(depth);
                let delimiter_depth = (paren_depth, bracket_depth);
                if pending_controls.last().copied() != Some(delimiter_depth) {
                    pending_controls.push(delimiter_depth);
                }
            }
            next_kw += 1;
        }

        match bytes[i] {
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'{' => {
                // A closure or object literal inside a condition may open a
                // brace before the control-flow body. Only claim a brace
                // after delimiters have returned to the keyword's level.
                let opens_control = pending_controls.last().is_some_and(|&(paren, bracket)| {
                    paren_depth == paren && bracket_depth == bracket
                });
                brace_stack.push(opens_control);
                if opens_control {
                    pending_controls.pop();
                    control_depth += 1;
                }
            }
            b'}' => {
                if brace_stack.pop().unwrap_or(false) {
                    control_depth = control_depth.saturating_sub(1);
                }
            }
            b';' => {
                // A semicolon nested inside a condition's closure does not
                // finish the outer control statement. At the same delimiter
                // level it does finish a braceless statement.
                pending_controls
                    .retain(|&(paren, bracket)| paren_depth > paren || bracket_depth > bracket);
            }
            _ => {}
        }
        i += 1;
    }
    while next_kw < keywords.len() {
        let keyword = keywords[next_kw];
        let depth = if keyword.is_control_flow {
            control_depth + 1
        } else {
            control_depth.max(1)
        };
        cognitive += depth;
        if keyword.is_control_flow {
            max_depth = max_depth.max(depth);
        }
        next_kw += 1;
    }

    (max_depth, cognitive)
}

fn scan_indented_depth(
    content: &str,
    keywords: &[KeywordOccurrence],
    function_spans: &[FunctionSpan],
) -> (usize, usize) {
    if keywords.is_empty() {
        return (0, 0);
    }

    let mut line_starts = vec![0usize];
    for (offset, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(offset + 1);
        }
    }
    let line_for = |offset: usize| line_starts.partition_point(|&start| start <= offset);
    let indent_for = |line: usize| {
        let start = line_starts[line.saturating_sub(1)];
        content.as_bytes()[start..]
            .iter()
            .take_while(|&&byte| byte == b' ' || byte == b'\t')
            .map(|&byte| if byte == b'\t' { 4 } else { 1 })
            .sum::<usize>()
    };
    let group_for = |line: usize| {
        function_spans
            .iter()
            .position(|span| (span.start_line..=span.end_line).contains(&line))
            .unwrap_or(function_spans.len())
    };

    let positions: Vec<(usize, usize, bool)> = keywords
        .iter()
        .map(|keyword| {
            let line = line_for(keyword.offset);
            (group_for(line), indent_for(line), keyword.is_control_flow)
        })
        .collect();
    let mut indent_levels = vec![Vec::new(); function_spans.len() + 1];
    for &(group, indent, is_control_flow) in &positions {
        if is_control_flow {
            indent_levels[group].push(indent);
        }
    }
    for levels in &mut indent_levels {
        levels.sort_unstable();
        levels.dedup();
    }

    let mut max_depth = 0;
    let mut cognitive = 0;
    for (group, indent, is_control_flow) in positions {
        let depth = indent_levels[group]
            .partition_point(|level| *level <= indent)
            .max(1);
        cognitive += depth;
        if is_control_flow {
            max_depth = max_depth.max(depth);
        }
    }
    (max_depth, cognitive)
}

/// Historical v1 depth counted every bracket pair, including function calls,
/// array literals, and the function body itself. Keep it isolated from the
/// control-flow-only v2 depth so both models remain reproducible.
fn scan_legacy_bracket_depth(content: &str) -> usize {
    let mut max_depth = 0usize;
    let mut current_depth = 0usize;
    for byte in content.bytes() {
        match byte {
            b'{' | b'(' | b'[' => {
                current_depth += 1;
                max_depth = max_depth.max(current_depth);
            }
            b'}' | b')' | b']' => current_depth = current_depth.saturating_sub(1),
            _ => {}
        }
    }
    max_depth
}

fn is_control_flow_keyword(keyword: &str) -> bool {
    matches!(
        keyword.to_ascii_lowercase().as_str(),
        "if" | "elif"
            | "elseif"
            | "elsif"
            | "else"
            | "unless"
            | "for"
            | "foreach"
            | "while"
            | "until"
            | "loop"
            | "match"
            | "switch"
            | "select"
            | "case"
            | "when"
            | "catch"
            | "except"
            | "rescue"
            | "with"
            | "try"
            | "repeat"
            | "cond"
            | "receive"
            | "guard"
            | "do"
            | "perform"
            | "evaluate"
    )
}

fn function_spans_in(content: &str, re: &Regex, lang: &Language) -> Vec<FunctionSpan> {
    function_spans_in_mode(content, re, lang, true)
}

fn legacy_function_spans_in(content: &str, re: &Regex, lang: &Language) -> Vec<FunctionSpan> {
    function_spans_in_mode(content, re, lang, false)
}

fn function_spans_in_mode(
    content: &str,
    re: &Regex,
    lang: &Language,
    reject_control_signatures: bool,
) -> Vec<FunctionSpan> {
    let matches: Vec<(usize, usize, String)> = re
        .find_iter(content)
        .filter_map(|m| {
            if reject_control_signatures && is_control_signature(m.as_str()) {
                return None;
            }
            // Patterns like `(?m)^\s*fn ...` swallow preceding blank lines;
            // anchor the span at the signature rather than that whitespace.
            let lead_ws = m.as_str().len() - m.as_str().trim_start().len();
            Some((
                m.start() + lead_ws,
                m.end(),
                trailing_identifier(m.as_str()),
            ))
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

fn is_control_signature(matched: &str) -> bool {
    let first = matched
        .trim_start()
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .find(|part| !part.is_empty())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "if" | "else" | "for" | "while" | "switch" | "catch" | "with"
    )
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

fn uses_brace_control_flow(language: &str) -> bool {
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
    use crate::language::LanguageRegistry;

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
        assert!(complexity.functions_measured);
        assert!(complexity.control_flow_measured);
    }

    #[test]
    fn test_missing_rules_are_reported_as_unmeasured() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = Language {
            name: "Custom".to_string(),
            extensions: vec![".custom".to_string()],
            ..Default::default()
        };

        let complexity = analyzer.analyze("value = 1\n", &lang);

        assert!(!complexity.functions_measured);
        assert!(!complexity.control_flow_measured);
    }

    #[test]
    fn test_javascript_and_typescript_common_function_forms() {
        let registry = LanguageRegistry::with_builtin().unwrap();
        let analyzer = ComplexityAnalyzer::new();
        let source = r#"
function declared(value) { return value; }
class Service {
  constructor(value) { this.value = value; }
  async load<T>(value: T): Promise<T> { return value; }
  method(value) { return value; }
  field = (value) => { return value; };
}
const expression = value => value * 2;
const values = [1, 2].map((value) => value + 1);
if (values.length) { consume(values); }
"#;

        for language in ["JavaScript", "TypeScript"] {
            let lang = registry.get(language).unwrap();
            let complexity = analyzer.analyze(source, &lang);
            assert_eq!(
                complexity.functions, 7,
                "{language} should recognize declarations, methods, arrows, and callbacks"
            );
            assert_eq!(
                complexity.legacy_functions,
                Some(2),
                "{language} v1 must retain the historical function matcher"
            );
            assert_eq!(complexity.legacy_cyclomatic, Some(3));
            assert_eq!(complexity.legacy_max_depth, Some(2));
            assert_eq!(complexity.legacy_avg_func_lines, Some(1.0));
            assert!(complexity.functions_measured);
            assert!(complexity.control_flow_measured);
        }
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
        assert_eq!(complexity.max_depth, 0);
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
        assert_eq!(
            complexity.max_depth, 0,
            "ordinary brackets must not count as control-flow nesting"
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
        assert_eq!(complexity.max_depth, 1);
    }

    #[test]
    fn test_max_depth_ignores_calls_arrays_and_literals() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = r#"
fn f() {
    let values = [call(one(), two([1, 2, 3])), other()];
    let map = Thing { values, nested: Some(call(three())) };
}
"#;

        let complexity = analyzer.analyze(content, &lang);

        assert_eq!(complexity.max_depth, 0);
        assert!(complexity.legacy_max_depth.unwrap() > complexity.max_depth);
    }

    #[test]
    fn test_max_depth_counts_nested_control_flow_only() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = r#"
impl Worker {
    fn run() {
        if ready() {
            for item in items() {
                while item.pending() {
                    process(item);
                }
            }
        }
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);

        assert_eq!(complexity.max_depth, 3);
    }

    #[test]
    fn test_control_block_skips_closure_brace_in_condition() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = r#"
fn run(values: &[i32]) {
    if values.iter().any(|value| { value > 0 }) {
        for value in values {
            consume(value);
        }
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);

        assert_eq!(complexity.max_depth, 2);
    }

    #[test]
    fn test_indentation_depth_counts_control_levels() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = Language {
            name: "Python".to_string(),
            extensions: vec![".py".to_string()],
            line_comments: vec!["#".to_string()],
            function_pattern: Some(r"(?m)^\s*def\s+\w+".to_string()),
            complexity_keywords: vec![
                "if".to_string(),
                "for".to_string(),
                "while".to_string(),
                "and".to_string(),
            ],
            ..Default::default()
        };
        let content = "def run():\n    if ready:\n        for item in items:\n            while item.pending:\n                work(item)\n";

        let complexity = analyzer.analyze(content, &lang);

        assert_eq!(complexity.max_depth, 3);
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
    fn test_cognitive_condition_operators_use_pending_control_depth() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();
        let content = "fn f() { if outer() { if left() && right() { work(); } } }\n";

        let complexity = analyzer.analyze(content, &lang);

        // outer if = 1, inner if = 2, && in the inner condition = 2.
        assert_eq!(complexity.cognitive, 5);
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
        assert_eq!(complexity.max_depth, 3);
    }
}
