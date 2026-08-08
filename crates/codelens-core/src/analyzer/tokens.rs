//! LLM token estimation.
//!
//! Rough byte-per-token coefficients for modern BPE tokenizers (Claude,
//! GPT, Gemini families all land in the same ballpark on source code:
//! roughly 3-4 bytes per token, denser for markup, sparser for prose).
//! These are rule-of-thumb estimates, not tokenizer-exact counts — output
//! should always present them with a "≈".

/// Bytes-per-token coefficient for a language (case-insensitive name
/// as produced by the language registry).
pub fn bytes_per_token(language: &str) -> f64 {
    match language.to_ascii_lowercase().as_str() {
        // Prose tokenizes efficiently (whole words become single tokens).
        "markdown" | "text" | "restructuredtext" | "asciidoc" | "org" => 4.0,
        // Brace-and-symbol heavy systems languages.
        "rust" | "go" | "c" | "c++" | "c header" | "c++ header" | "zig" => 3.3,
        "javascript" | "typescript" | "jsx" | "tsx" => 3.4,
        // Config/data formats: much punctuation, short keys.
        "json" | "yaml" | "toml" | "ini" | "xml" => 3.0,
        "html" | "css" | "scss" | "sass" | "less" | "svg" => 3.1,
        _ => 3.5,
    }
}

/// Estimate the token count for `bytes` of `language` source.
pub fn estimate_tokens(language: &str, bytes: u64) -> u64 {
    (bytes as f64 / bytes_per_token(language)).round() as u64
}

/// A reference LLM context window for "does this fit" comparisons.
pub struct ContextWindow {
    pub label: &'static str,
    pub tokens: u64,
}

/// Common context windows, largest models people actually target.
pub const CONTEXT_WINDOWS: &[ContextWindow] = &[
    ContextWindow {
        label: "200K context (Claude, GPT)",
        tokens: 200_000,
    },
    ContextWindow {
        label: "1M context (Gemini, Claude Sonnet 1M beta)",
        tokens: 1_000_000,
    },
];

/// Format a token count compactly: 950, ≈12.3K, ≈4.5M.
pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("≈{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("≈{:.1}K", tokens as f64 / 1_000.0)
    } else {
        format!("≈{tokens}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_by_language() {
        // 3500 bytes of Rust at 3.3 bytes/token ≈ 1061 tokens.
        assert_eq!(estimate_tokens("Rust", 3500), 1061);
        // Markdown is sparser: 4.0 bytes/token.
        assert_eq!(estimate_tokens("Markdown", 4000), 1000);
        // Unknown language falls back to the default coefficient.
        assert_eq!(estimate_tokens("Brainfuck", 3500), 1000);
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(bytes_per_token("RUST"), bytes_per_token("rust"));
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(950), "≈950");
        assert_eq!(format_tokens(12_300), "≈12.3K");
        assert_eq!(format_tokens(4_500_000), "≈4.5M");
    }
}
