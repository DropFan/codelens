//! Token trie for fast multi-pattern matching.
//!
//! A 256-way trie that unifies all token types (comments, strings, keywords)
//! into a single lookup structure. Combined with a process mask (bloom filter
//! on first bytes), this skips ~90% of bytes in the hot loop.

/// Type of token matched by the trie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    LineComment,
    BlockCommentStart,
    StringDelimiter,
    DocStringDelimiter,
}

/// Result of a successful trie match.
///
/// Note: `advance` is set automatically by `TokenTrie::insert()` based on
/// the pattern length. Any value provided by the caller will be overwritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenMatch {
    pub token_type: TokenType,
    /// Closing byte sequence (e.g., `*/` for block comments, `"` for strings).
    /// None for line comments (they end at newline).
    pub close: Option<Vec<u8>>,
    /// How many bytes to advance past the matched token (set by insert).
    pub advance: usize,
    /// Whether the token content may span multiple lines
    /// (string delimiters only; docstrings are always multiline).
    pub multiline: bool,
}

impl TokenMatch {
    /// Create a token match with default flags (advance is set by `insert`).
    pub fn new(token_type: TokenType, close: Option<Vec<u8>>) -> Self {
        Self {
            token_type,
            close,
            advance: 0,
            multiline: false,
        }
    }
}

struct TrieNode {
    children: Vec<Option<Box<TrieNode>>>,
    token_match: Option<TokenMatch>,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: (0..256).map(|_| None).collect(),
            token_match: None,
        }
    }
}

/// Fast token lookup trie. All token types for a language are inserted into one trie.
///
/// `Debug` prints only the process mask (the full trie is too large to dump).
pub struct TokenTrie {
    root: TrieNode,
    mask: u8,
}

impl TokenTrie {
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(),
            mask: 0,
        }
    }

    pub fn insert(&mut self, pattern: &[u8], mut token_match: TokenMatch) {
        if pattern.is_empty() {
            return;
        }
        self.mask |= pattern[0];
        token_match.advance = pattern.len();

        let mut node = &mut self.root;
        for &byte in pattern {
            let idx = byte as usize;
            if node.children[idx].is_none() {
                node.children[idx] = Some(Box::new(TrieNode::new()));
            }
            node = node.children[idx].as_mut().unwrap();
        }
        node.token_match = Some(token_match);
    }

    /// Try to match a token at position `pos`. Returns longest match (greedy).
    pub fn match_at(&self, content: &[u8], pos: usize) -> Option<TokenMatch> {
        let mut node = &self.root;
        let mut last_match: Option<&TokenMatch> = None;

        for &byte in &content[pos..] {
            match &node.children[byte as usize] {
                Some(child) => {
                    node = child;
                    if node.token_match.is_some() {
                        last_match = node.token_match.as_ref();
                    }
                }
                None => break,
            }
        }

        last_match.cloned()
    }

    pub fn process_mask(&self) -> u8 {
        self.mask
    }
}

impl Default for TokenTrie {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TokenTrie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenTrie")
            .field("mask", &self.mask)
            .finish_non_exhaustive()
    }
}

/// Build a `TokenTrie` from a `Language` definition.
///
/// Inserts line comments, block comment starts (with close bytes),
/// string delimiters from the language's `string_delimiters` config
/// (falling back to single-line `"` and `'` when unconfigured), and
/// for Python/Ruby also triple-quote doc-string delimiters (`"""` and `'''`).
pub fn build_from_language(lang: &crate::language::Language) -> (TokenTrie, u8) {
    let mut trie = TokenTrie::new();

    // Line comments
    for lc in &lang.line_comments {
        trie.insert(lc.as_bytes(), TokenMatch::new(TokenType::LineComment, None));
    }

    // Block comments
    for (open, close) in &lang.block_comments {
        trie.insert(
            open.as_bytes(),
            TokenMatch::new(
                TokenType::BlockCommentStart,
                Some(close.as_bytes().to_vec()),
            ),
        );
    }

    // String delimiters: use the language config when present; otherwise
    // fall back to `"` and `'`, treated as single-line so an unclosed
    // quote cannot poison the rest of the file.
    if lang.string_delimiters.is_empty() {
        for delim in [b"\"".as_slice(), b"'".as_slice()] {
            trie.insert(
                delim,
                TokenMatch::new(TokenType::StringDelimiter, Some(delim.to_vec())),
            );
        }
    } else {
        for sd in &lang.string_delimiters {
            trie.insert(
                sd.start.as_bytes(),
                TokenMatch {
                    multiline: sd.multiline,
                    ..TokenMatch::new(TokenType::StringDelimiter, Some(sd.end.as_bytes().to_vec()))
                },
            );
        }
    }

    // Doc-string delimiters for Python and Ruby (longest-match wins over single quotes)
    let name = lang.name.as_str();
    if name == "Python" || name == "Ruby" {
        trie.insert(
            b"\"\"\"",
            TokenMatch {
                multiline: true,
                ..TokenMatch::new(TokenType::DocStringDelimiter, Some(b"\"\"\"".to_vec()))
            },
        );
        trie.insert(
            b"'''",
            TokenMatch {
                multiline: true,
                ..TokenMatch::new(TokenType::DocStringDelimiter, Some(b"'''".to_vec()))
            },
        );
    }

    let mask = trie.process_mask();
    (trie, mask)
}

/// Fast check: could this byte possibly start a token?
///
/// This is a bloom-filter-style check using a bitwise OR mask of all first
/// bytes of inserted patterns. False positives are expected and acceptable
/// (they just trigger a `match_at` call that returns `None`). False negatives
/// are impossible by construction (`mask |= first_byte` for every pattern).
#[inline(always)]
pub fn should_process(byte: u8, mask: u8) -> bool {
    byte & mask == byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_trie_matches_nothing() {
        let trie = TokenTrie::new();
        assert_eq!(trie.match_at(b"hello", 0), None);
    }

    #[test]
    fn test_single_line_comment() {
        let mut trie = TokenTrie::new();
        trie.insert(b"//", TokenMatch::new(TokenType::LineComment, None));
        let m = trie.match_at(b"// comment", 0).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);
        assert_eq!(m.advance, 2);
        assert!(m.close.is_none());
    }

    #[test]
    fn test_block_comment() {
        let mut trie = TokenTrie::new();
        trie.insert(
            b"/*",
            TokenMatch::new(TokenType::BlockCommentStart, Some(b"*/".to_vec())),
        );
        let m = trie.match_at(b"/* block */", 0).unwrap();
        assert_eq!(m.token_type, TokenType::BlockCommentStart);
        assert_eq!(m.advance, 2);
        assert_eq!(m.close.as_deref(), Some(b"*/".as_slice()));
    }

    #[test]
    fn test_no_match_at_wrong_position() {
        let mut trie = TokenTrie::new();
        trie.insert(b"//", TokenMatch::new(TokenType::LineComment, None));
        assert_eq!(trie.match_at(b"x // y", 0), None);
        let m = trie.match_at(b"x // y", 2).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);
    }

    #[test]
    fn test_string_delimiter() {
        let mut trie = TokenTrie::new();
        trie.insert(
            b"\"",
            TokenMatch::new(TokenType::StringDelimiter, Some(b"\"".to_vec())),
        );
        let m = trie.match_at(b"\"hello\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);
        assert_eq!(m.close.as_deref(), Some(b"\"".as_slice()));
    }

    #[test]
    fn test_process_mask_filters_correctly() {
        let mut trie = TokenTrie::new();
        trie.insert(b"//", TokenMatch::new(TokenType::LineComment, None));
        trie.insert(
            b"\"",
            TokenMatch::new(TokenType::StringDelimiter, Some(b"\"".to_vec())),
        );
        let mask = trie.process_mask();
        assert!(should_process(b'/', mask));
        assert!(should_process(b'"', mask));
        // Letters should generally not pass (bloom filter allows some false positives)
        assert!(!should_process(b'a', mask));
    }

    #[test]
    fn test_longer_match_wins() {
        let mut trie = TokenTrie::new();
        trie.insert(
            b"\"",
            TokenMatch::new(TokenType::StringDelimiter, Some(b"\"".to_vec())),
        );
        trie.insert(
            b"\"\"\"",
            TokenMatch::new(TokenType::DocStringDelimiter, Some(b"\"\"\"".to_vec())),
        );
        let m = trie.match_at(b"\"\"\"hello\"\"\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::DocStringDelimiter);
        assert_eq!(m.advance, 3);
    }

    #[test]
    fn test_build_from_rust_language() {
        use crate::language::Language;

        let lang = Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            nested_comments: true,
            ..Default::default()
        };
        let (trie, mask) = build_from_language(&lang);

        let m = trie.match_at(b"// comment", 0).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);

        let m = trie.match_at(b"/* block */", 0).unwrap();
        assert_eq!(m.token_type, TokenType::BlockCommentStart);
        assert_eq!(m.close.as_deref(), Some(b"*/".as_slice()));

        let m = trie.match_at(b"\"hello\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);

        assert_ne!(mask, 0);
    }

    #[test]
    fn test_configured_string_delimiters_override_defaults() {
        use crate::language::{Language, StringDelimiter};

        // Rust 风格配置：只有 "，' 不是字符串定界符
        let lang = Language {
            name: "Rust".to_string(),
            line_comments: vec!["//".to_string()],
            string_delimiters: vec![StringDelimiter {
                start: "\"".to_string(),
                end: "\"".to_string(),
                escape: Some("\\".to_string()),
                multiline: true,
            }],
            ..Default::default()
        };
        let (trie, _mask) = build_from_language(&lang);

        // ' 不再被识别为字符串定界符
        assert_eq!(trie.match_at(b"'a'", 0), None);

        let m = trie.match_at(b"\"s\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);
        assert!(m.multiline);
    }

    #[test]
    fn test_default_string_delimiters_are_single_line() {
        use crate::language::Language;

        let lang = Language {
            name: "X".to_string(),
            ..Default::default()
        };
        let (trie, _mask) = build_from_language(&lang);

        let m = trie.match_at(b"\"s\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);
        assert!(!m.multiline);

        let m = trie.match_at(b"'s'", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);
        assert!(!m.multiline);
    }

    #[test]
    fn test_build_from_python_language() {
        use crate::language::Language;

        let lang = Language {
            name: "Python".to_string(),
            extensions: vec![".py".to_string()],
            line_comments: vec!["#".to_string()],
            ..Default::default()
        };
        let (trie, _mask) = build_from_language(&lang);

        let m = trie.match_at(b"# comment", 0).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);

        let m = trie.match_at(b"\"\"\"docstring\"\"\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::DocStringDelimiter);
        assert_eq!(m.close.as_deref(), Some(b"\"\"\"".as_slice()));

        let m = trie.match_at(b"'''docstring'''", 0).unwrap();
        assert_eq!(m.token_type, TokenType::DocStringDelimiter);
        assert_eq!(m.close.as_deref(), Some(b"'''".as_slice()));
    }
}
