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

        for i in pos..content.len() {
            let idx = content[i] as usize;
            match &node.children[idx] {
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
        trie.insert(b"//", TokenMatch {
            token_type: TokenType::LineComment,
            close: None,
            advance: 0,
        });
        let m = trie.match_at(b"// comment", 0).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);
        assert_eq!(m.advance, 2);
        assert!(m.close.is_none());
    }

    #[test]
    fn test_block_comment() {
        let mut trie = TokenTrie::new();
        trie.insert(b"/*", TokenMatch {
            token_type: TokenType::BlockCommentStart,
            close: Some(b"*/".to_vec()),
            advance: 0,
        });
        let m = trie.match_at(b"/* block */", 0).unwrap();
        assert_eq!(m.token_type, TokenType::BlockCommentStart);
        assert_eq!(m.advance, 2);
        assert_eq!(m.close.as_deref(), Some(b"*/".as_slice()));
    }

    #[test]
    fn test_no_match_at_wrong_position() {
        let mut trie = TokenTrie::new();
        trie.insert(b"//", TokenMatch {
            token_type: TokenType::LineComment,
            close: None,
            advance: 0,
        });
        assert_eq!(trie.match_at(b"x // y", 0), None);
        let m = trie.match_at(b"x // y", 2).unwrap();
        assert_eq!(m.token_type, TokenType::LineComment);
    }

    #[test]
    fn test_string_delimiter() {
        let mut trie = TokenTrie::new();
        trie.insert(b"\"", TokenMatch {
            token_type: TokenType::StringDelimiter,
            close: Some(b"\"".to_vec()),
            advance: 0,
        });
        let m = trie.match_at(b"\"hello\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::StringDelimiter);
        assert_eq!(m.close.as_deref(), Some(b"\"".as_slice()));
    }

    #[test]
    fn test_process_mask_filters_correctly() {
        let mut trie = TokenTrie::new();
        trie.insert(b"//", TokenMatch {
            token_type: TokenType::LineComment,
            close: None,
            advance: 0,
        });
        trie.insert(b"\"", TokenMatch {
            token_type: TokenType::StringDelimiter,
            close: Some(b"\"".to_vec()),
            advance: 0,
        });
        let mask = trie.process_mask();
        assert!(should_process(b'/', mask));
        assert!(should_process(b'"', mask));
        // Letters should generally not pass (bloom filter allows some false positives)
        assert!(!should_process(b'a', mask));
    }

    #[test]
    fn test_longer_match_wins() {
        let mut trie = TokenTrie::new();
        trie.insert(b"\"", TokenMatch {
            token_type: TokenType::StringDelimiter,
            close: Some(b"\"".to_vec()),
            advance: 0,
        });
        trie.insert(b"\"\"\"", TokenMatch {
            token_type: TokenType::DocStringDelimiter,
            close: Some(b"\"\"\"".to_vec()),
            advance: 0,
        });
        let m = trie.match_at(b"\"\"\"hello\"\"\"", 0).unwrap();
        assert_eq!(m.token_type, TokenType::DocStringDelimiter);
        assert_eq!(m.advance, 3);
    }
}
