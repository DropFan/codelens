//! Byte-level state machine for counting code, comment, and blank lines.

use crate::analyzer::stats::LineStats;
use crate::analyzer::trie::{should_process, TokenTrie, TokenType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Blank,
    Code,
    LineComment,
    LineCommentAfterCode,
    BlockComment,
    BlockCommentAfterCode,
    InString,
    InDocString,
}

/// Count code/comment/blank lines in a single pass over raw bytes.
pub fn count_stats(content: &[u8], trie: &TokenTrie, mask: u8) -> LineStats {
    let len = content.len();
    if len == 0 {
        return LineStats::default();
    }

    let mut stats = LineStats::default();
    let mut state = State::Blank;
    let mut close_bytes: Vec<u8> = Vec::new();
    let mut index: usize = 0;

    while index < len {
        let byte = content[index];

        // Newline: classify line and reset
        if byte == b'\n' {
            stats.total += 1;
            classify_line(&state, &mut stats);
            state = match state {
                State::BlockComment | State::BlockCommentAfterCode => State::BlockComment,
                State::InString => State::InString,
                State::InDocString => State::InDocString,
                _ => State::Blank,
            };
            index += 1;
            continue;
        }

        match state {
            State::Blank => {
                if byte.is_ascii_whitespace() {
                    index += 1;
                    continue;
                }
                if should_process(byte, mask) {
                    if let Some(m) = trie.match_at(content, index) {
                        match m.token_type {
                            TokenType::LineComment => {
                                state = State::LineComment;
                                index += m.advance;
                                continue;
                            }
                            TokenType::BlockCommentStart => {
                                close_bytes = m.close.unwrap_or_default();
                                state = State::BlockComment;
                                index += m.advance;
                                continue;
                            }
                            TokenType::StringDelimiter => {
                                close_bytes = m.close.unwrap_or_default();
                                state = State::InString;
                                index += m.advance;
                                continue;
                            }
                            TokenType::DocStringDelimiter => {
                                close_bytes = m.close.unwrap_or_default();
                                state = State::InDocString;
                                index += m.advance;
                                continue;
                            }
                        }
                    }
                }
                state = State::Code;
                index += 1;
            }

            State::Code => {
                if should_process(byte, mask) {
                    if let Some(m) = trie.match_at(content, index) {
                        match m.token_type {
                            TokenType::LineComment => {
                                state = State::LineCommentAfterCode;
                                index += m.advance;
                                continue;
                            }
                            TokenType::BlockCommentStart => {
                                close_bytes = m.close.unwrap_or_default();
                                state = State::BlockCommentAfterCode;
                                index += m.advance;
                                continue;
                            }
                            TokenType::StringDelimiter => {
                                close_bytes = m.close.unwrap_or_default();
                                state = State::InString;
                                index += m.advance;
                                continue;
                            }
                            TokenType::DocStringDelimiter => {
                                // Triple-quote after code = string assignment, not docstring
                                close_bytes = m.close.unwrap_or_default();
                                state = State::InString;
                                index += m.advance;
                                continue;
                            }
                        }
                    }
                }
                index += 1;
            }

            State::LineComment | State::LineCommentAfterCode => {
                // Skip until newline (handled at top of loop)
                index += 1;
            }

            State::BlockComment | State::BlockCommentAfterCode => {
                if content_matches_at(content, index, &close_bytes) {
                    index += close_bytes.len();
                    state = match state {
                        State::BlockCommentAfterCode => State::Code,
                        // Line started blank; remember this was a comment line
                        _ => State::LineComment,
                    };
                    continue;
                }
                index += 1;
            }

            State::InString => {
                if byte == b'\\' {
                    index += 2; // skip escaped char
                    continue;
                }
                if content_matches_at(content, index, &close_bytes) {
                    index += close_bytes.len();
                    state = State::Code;
                    continue;
                }
                index += 1;
            }

            State::InDocString => {
                if content_matches_at(content, index, &close_bytes) {
                    index += close_bytes.len();
                    // Docstring closed; remember this line had a docstring
                    state = State::LineComment;
                    continue;
                }
                index += 1;
            }
        }
    }

    // Handle last line without trailing newline
    if content[len - 1] != b'\n' {
        stats.total += 1;
        classify_line(&state, &mut stats);
    }

    stats
}

#[inline(always)]
fn classify_line(state: &State, stats: &mut LineStats) {
    match state {
        State::Blank => stats.blank += 1,
        State::Code
        | State::InString
        | State::LineCommentAfterCode
        | State::BlockCommentAfterCode => {
            stats.code += 1;
        }
        State::LineComment | State::BlockComment | State::InDocString => {
            stats.comment += 1;
        }
    }
}

#[inline(always)]
fn content_matches_at(content: &[u8], pos: usize, pattern: &[u8]) -> bool {
    if pos + pattern.len() > content.len() {
        return false;
    }
    &content[pos..pos + pattern.len()] == pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::trie::build_from_language;
    use crate::language::Language;

    fn rust_lang() -> Language {
        Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            nested_comments: true,
            ..Default::default()
        }
    }

    fn python_lang() -> Language {
        Language {
            name: "Python".to_string(),
            extensions: vec![".py".to_string()],
            line_comments: vec!["#".to_string()],
            ..Default::default()
        }
    }

    fn count(content: &str, lang: &Language) -> LineStats {
        let (trie, mask) = build_from_language(lang);
        count_stats(content.as_bytes(), &trie, mask)
    }

    #[test]
    fn test_pure_code() {
        let stats = count("fn main() {\n    println!(\"hello\");\n}\n", &rust_lang());
        assert_eq!(stats.total, 3);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.comment, 0);
        assert_eq!(stats.blank, 0);
    }

    #[test]
    fn test_blank_lines() {
        let stats = count("fn main() {\n\n    let x = 1;\n\n}\n", &rust_lang());
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.blank, 2);
    }

    #[test]
    fn test_line_comments() {
        let stats = count("// comment\nfn main() {}\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_block_comment_single_line() {
        let stats = count("/* comment */\nfn main() {}\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_block_comment_multi_line() {
        let stats = count(
            "/*\n * Multi-line\n * comment\n */\nfn main() {}\n",
            &rust_lang(),
        );
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 4);
    }

    #[test]
    fn test_code_then_line_comment() {
        let stats = count("let x = 1; // init\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_string_with_comment_chars() {
        let stats = count("let s = \"// not a comment\";\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_string_with_block_comment_chars() {
        let stats = count("let s = \"/* not a comment */\";\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_escaped_quote_in_string() {
        let stats = count("let s = \"hello \\\" world\";\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_multiline_string() {
        let stats = count("let s = \"hello\nworld\";\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_python_line_comment() {
        let stats = count("# comment\nx = 1\n", &python_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_python_docstring_single_line() {
        let stats = count(
            "def foo():\n    \"\"\"docstring\"\"\"\n    pass\n",
            &python_lang(),
        );
        assert_eq!(stats.total, 3);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_python_multiline_docstring() {
        let stats = count(
            "def foo():\n    \"\"\"\n    Multi-line\n    docstring\n    \"\"\"\n    pass\n",
            &python_lang(),
        );
        assert_eq!(stats.total, 6);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 4);
    }

    #[test]
    fn test_empty_content() {
        let stats = count("", &rust_lang());
        assert_eq!(stats.total, 0);
        assert_eq!(stats.code, 0);
    }

    #[test]
    fn test_single_newline() {
        let stats = count("\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.blank, 1);
    }

    #[test]
    fn test_no_trailing_newline() {
        let stats = count("fn main() {}", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_code_with_inline_block_comment() {
        // Code with inline block comment should be code
        let stats = count("let x = /* value */ 42;\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_char_literal_not_confused_with_string() {
        // Rust char literal 'a' should not leave string state open
        let stats = count("let c = 'a';\n// comment\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }
}
