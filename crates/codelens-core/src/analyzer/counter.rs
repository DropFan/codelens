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
    /// 块注释/文档字符串在行内闭合后继续扫描：
    /// 若行尾前再无代码则整行算注释，出现代码则转为 Code。
    AfterBlockComment,
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
    // 当前字符串定界符是否允许跨行（非跨行的未闭合字符串在行尾复位）
    let mut string_multiline = false;
    // 当前字符串定界符的转义字节（None 表示无转义，如 shell 单引号）
    let mut string_escape: Option<u8> = None;
    // 当前块注释的开启序列、是否可嵌套及嵌套深度
    let mut open_bytes: Vec<u8> = Vec::new();
    let mut comment_nested = false;
    let mut comment_depth: usize = 0;
    // 当前块注释的开/闭序列是否要求出现在列 0（Ruby =begin/=end）
    let mut comment_line_start_only = false;
    // 行首标记：换行置 true，消费任何其他字节后清 false
    let mut at_line_start = true;
    let mut index: usize = 0;

    while index < len {
        let byte = content[index];

        // Newline: classify line and reset
        if byte == b'\n' {
            stats.total += 1;
            classify_line(&state, &mut stats);
            state = match state {
                State::BlockComment | State::BlockCommentAfterCode => State::BlockComment,
                State::InString if string_multiline => State::InString,
                State::InDocString => State::InDocString,
                _ => State::Blank,
            };
            at_line_start = true;
            index += 1;
            continue;
        }

        // 本字节是否位于列 0（本轮消费后即失效）
        let line_start = at_line_start;
        at_line_start = false;

        match state {
            State::Blank | State::AfterBlockComment => {
                if byte.is_ascii_whitespace() {
                    index += 1;
                    continue;
                }
                if should_process(byte, mask) {
                    // 行首约束的 token（如 Ruby =begin）不在行中生效
                    if let Some(m) = trie
                        .match_at(content, index)
                        .filter(|m| !m.line_start_only || line_start)
                    {
                        match m.token_type {
                            TokenType::LineComment => {
                                state = State::LineComment;
                                index += m.advance;
                                continue;
                            }
                            TokenType::BlockCommentStart => {
                                close_bytes = m.close.unwrap_or_default();
                                open_bytes = content[index..index + m.advance].to_vec();
                                comment_nested = m.nested;
                                comment_depth = 0;
                                comment_line_start_only = m.line_start_only;
                                state = State::BlockComment;
                                index += m.advance;
                                continue;
                            }
                            TokenType::StringDelimiter => {
                                close_bytes = m.close.unwrap_or_default();
                                string_multiline = m.multiline;
                                string_escape = m.escape;
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
                    // 行首约束的 token（如 Ruby =begin）不在行中生效
                    if let Some(m) = trie
                        .match_at(content, index)
                        .filter(|m| !m.line_start_only || line_start)
                    {
                        match m.token_type {
                            TokenType::LineComment => {
                                state = State::LineCommentAfterCode;
                                index += m.advance;
                                continue;
                            }
                            TokenType::BlockCommentStart => {
                                close_bytes = m.close.unwrap_or_default();
                                open_bytes = content[index..index + m.advance].to_vec();
                                comment_nested = m.nested;
                                comment_depth = 0;
                                comment_line_start_only = m.line_start_only;
                                state = State::BlockCommentAfterCode;
                                index += m.advance;
                                continue;
                            }
                            TokenType::StringDelimiter => {
                                close_bytes = m.close.unwrap_or_default();
                                string_multiline = m.multiline;
                                string_escape = m.escape;
                                state = State::InString;
                                index += m.advance;
                                continue;
                            }
                            TokenType::DocStringDelimiter => {
                                // Triple-quote after code = string assignment, not docstring
                                close_bytes = m.close.unwrap_or_default();
                                string_multiline = m.multiline;
                                string_escape = m.escape;
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
                // 行首约束的闭合序列（如 Ruby =end）只在列 0 生效
                let anchored = !comment_line_start_only || line_start;
                if anchored && content_matches_at(content, index, &close_bytes) {
                    index += close_bytes.len();
                    // 嵌套注释：先弹出内层，深度归零才真正闭合
                    if comment_depth > 0 {
                        comment_depth -= 1;
                        continue;
                    }
                    state = match state {
                        State::BlockCommentAfterCode => State::Code,
                        // 行首开始的块注释闭合后继续扫描，
                        // 行尾按是否出现过代码再分类
                        _ => State::AfterBlockComment,
                    };
                    continue;
                }
                if comment_nested && anchored && content_matches_at(content, index, &open_bytes) {
                    comment_depth += 1;
                    index += open_bytes.len();
                    continue;
                }
                index += 1;
            }

            State::InString => {
                if string_escape == Some(byte) {
                    // 被转义的换行仍是物理行边界：只前进 1，
                    // 让换行走行首的正常分类路径
                    if index + 1 < len && content[index + 1] == b'\n' {
                        index += 1;
                    } else {
                        index += 2; // skip escaped char
                    }
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
                    // docstring 闭合后继续扫描，同行代码算 code
                    state = State::AfterBlockComment;
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
        State::LineComment
        | State::BlockComment
        | State::AfterBlockComment
        | State::InDocString => {
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
    use crate::language::{Language, LanguageRegistry, StringDelimiter};

    fn rust_lang() -> Language {
        Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            // 与 languages.toml 保持一致：Rust 只有 " 定界符（' 是生命周期/char）
            string_delimiters: vec![StringDelimiter {
                start: "\"".to_string(),
                end: "\"".to_string(),
                escape: Some("\\".to_string()),
                multiline: true,
            }],
            nested_comments: true,
            ..Default::default()
        }
    }

    fn yaml_lang() -> Language {
        Language {
            name: "YAML".to_string(),
            extensions: vec![".yml".to_string()],
            line_comments: vec!["#".to_string()],
            string_delimiters: vec![
                StringDelimiter {
                    start: "\"".to_string(),
                    end: "\"".to_string(),
                    escape: Some("\\".to_string()),
                    multiline: false,
                },
                StringDelimiter {
                    start: "'".to_string(),
                    end: "'".to_string(),
                    escape: None,
                    multiline: false,
                },
            ],
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

    fn ruby_lang() -> Language {
        Language {
            name: "Ruby".to_string(),
            extensions: vec![".rb".to_string()],
            line_comments: vec!["#".to_string()],
            block_comments: vec![("=begin".to_string(), "=end".to_string())],
            string_delimiters: vec![
                StringDelimiter {
                    start: "\"".to_string(),
                    end: "\"".to_string(),
                    escape: Some("\\".to_string()),
                    multiline: false,
                },
                StringDelimiter {
                    start: "'".to_string(),
                    end: "'".to_string(),
                    escape: Some("\\".to_string()),
                    multiline: false,
                },
            ],
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

    #[test]
    fn test_code_after_inline_block_comment_close() {
        // 块注释在行内闭合后，同行的代码必须算 code（对齐 scc/tokei）
        let stats = count("/* c */ int x = 1;\nint y = 2;\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_inline_block_comment_close_then_whitespace_only() {
        // 闭合后只剩空白 → 仍是注释行
        let stats = count("/* c */   \nlet x = 1;\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_inline_block_comment_close_then_line_comment() {
        // 闭合后接行注释 → 仍是注释行
        let stats = count("/* a */ // b\n", &rust_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.comment, 1);
        assert_eq!(stats.code, 0);
    }

    #[test]
    fn test_code_after_docstring_close_same_line() {
        // docstring 行内闭合后出现代码 → code
        let stats = count("\"\"\"doc\"\"\" x = 1\n", &python_lang());
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_escaped_newline_in_string_counts_both_lines() {
        // 字符串内的 \<换行> 是续行，但物理行数必须都被统计
        let stats = count("let s = \"a\\\nb\";\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 2);
    }

    #[test]
    fn test_no_escape_delimiter_backslash_is_literal() {
        // Go 反引号 raw string：\ 是普通字符，不能吞掉闭合定界符
        let go = Language {
            name: "Go".to_string(),
            line_comments: vec!["//".to_string()],
            string_delimiters: vec![
                StringDelimiter {
                    start: "\"".to_string(),
                    end: "\"".to_string(),
                    escape: Some("\\".to_string()),
                    multiline: false,
                },
                StringDelimiter {
                    start: "`".to_string(),
                    end: "`".to_string(),
                    escape: None,
                    multiline: true,
                },
            ],
            ..Default::default()
        };
        let stats = count("s := `a\\`\n// comment\n", &go);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_nested_block_comments() {
        // Rust 支持嵌套块注释：内层 */ 不能提前闭合外层
        let stats = count(
            "/* outer /* inner */\nstill comment */\nlet x = 1;\n",
            &rust_lang(),
        );
        assert_eq!(stats.total, 3);
        assert_eq!(stats.comment, 2);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_nested_block_comment_inline() {
        // 同一行内嵌套开启并全部闭合
        let stats = count("/* a /* b */ c */ let x = 1;\n// done\n", &rust_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_non_nested_language_closes_at_first_end() {
        // C 不支持嵌套：第一个 */ 即闭合
        let c = Language {
            name: "C".to_string(),
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            nested_comments: false,
            ..Default::default()
        };
        let stats = count("/* outer /* inner */ int x = 1;\n", &c);
        assert_eq!(stats.total, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_ruby_begin_end_only_at_line_start() {
        // =begin/=end 只在列 0 生效，行中出现不触发块注释
        let stats = count(
            "x=beginning_of_day\ny = 1\n# comment\nz=end_of_day\nw = 2\n",
            &ruby_lang(),
        );
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 4);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_ruby_begin_end_block_comment() {
        // 列 0 的 =begin/=end 正常构成块注释
        let stats = count("=begin\ncomment here\n=end\nx = 1\n", &ruby_lang());
        assert_eq!(stats.total, 4);
        assert_eq!(stats.comment, 3);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_ruby_end_marker_mid_line_does_not_close() {
        // 块注释内部行中的 =end 不闭合（Ruby 要求列 0）
        let stats = count("=begin\nnot the =end of it\n=end\nx = 1\n", &ruby_lang());
        assert_eq!(stats.total, 4);
        assert_eq!(stats.comment, 3);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn test_ruby_indented_begin_does_not_open() {
        // 缩进的 =begin 不是块注释
        let stats = count("  =begin\nx = 1\n", &ruby_lang());
        assert_eq!(stats.total, 2);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 0);
    }

    #[test]
    fn test_rust_lifetime_apostrophe_not_string() {
        // 生命周期的 ' 不是字符串定界符，后续注释不能被误判为代码
        let stats = count(
            "fn f() -> &'static str {\n    \"x\"\n}\n// comment 1\n// comment 2\n",
            &rust_lang(),
        );
        assert_eq!(stats.total, 5);
        assert_eq!(stats.code, 3);
        assert_eq!(stats.comment, 2);
    }

    #[test]
    fn test_yaml_apostrophe_does_not_poison_file() {
        // 普通文本里的撇号不能把后续注释拖进字符串状态
        let stats = count(
            "title: Tiger's guide\n# a comment\nkey: value\n",
            &yaml_lang(),
        );
        assert_eq!(stats.total, 3);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 1);
    }

    #[test]
    fn test_unclosed_default_quote_resets_at_newline() {
        // 未配置 string_delimiters 的语言：默认 "/' 定界符不跨行，
        // 未闭合引号只影响当前行
        let lang = Language {
            name: "X".to_string(),
            line_comments: vec!["//".to_string()],
            ..Default::default()
        };
        let stats = count("let s = 'oops\n// comment\n\ncode();\n", &lang);
        assert_eq!(stats.total, 4);
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 1);
        assert_eq!(stats.blank, 1);
    }

    #[test]
    fn test_builtin_rust_lifetime_acceptance() {
        // 验收用例：真实 languages.toml 的 Rust 配置
        let registry = LanguageRegistry::with_builtin().unwrap();
        let lang = registry.get("Rust").unwrap();
        let (trie, mask) = lang.tokens();
        let stats = count_stats(
            "fn f() -> &'static str {\n    \"x\"\n}\n// comment 1\n// comment 2\n".as_bytes(),
            trie,
            *mask,
        );
        assert_eq!(stats.code, 3);
        assert_eq!(stats.comment, 2);
    }

    #[test]
    fn test_builtin_yaml_apostrophe_acceptance() {
        // 验收用例：真实 languages.toml 的 YAML 配置
        let registry = LanguageRegistry::with_builtin().unwrap();
        let lang = registry.get("YAML").unwrap();
        let (trie, mask) = lang.tokens();
        let stats = count_stats(
            "title: Tiger's guide\n# a comment\nkey: value\n".as_bytes(),
            trie,
            *mask,
        );
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comment, 1);
    }
}
