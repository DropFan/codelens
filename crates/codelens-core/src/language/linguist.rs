//! GitHub linguist attribute support (.gitattributes).
//!
//! Honors the three attributes that affect what GitHub counts as code:
//! `linguist-language=X` (language override), `linguist-vendored`, and
//! `linguist-generated` (both exclude files; a `-` prefix or `=false`
//! re-includes them). Only the root .gitattributes is read.
//!
//! Pattern semantics follow .gitignore matching (no negated `!` patterns
//! exist in .gitattributes): a pattern without a slash matches at any
//! depth, a slash anchors it to the root.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};

/// One attribute assignment: pattern plus on/off state.
struct AttrRule {
    matcher: GlobMatcher,
    /// false for `-linguist-x` / `linguist-x=false`.
    enabled: bool,
}

/// Parsed linguist attributes from a .gitattributes file.
pub struct LinguistAttributes {
    root: PathBuf,
    /// (pattern, language) pairs in file order; the last match wins.
    overrides: Vec<(GlobMatcher, String)>,
    vendored: Vec<AttrRule>,
    generated: Vec<AttrRule>,
}

impl LinguistAttributes {
    /// Load linguist attributes from `<root>/.gitattributes`.
    /// Returns None when the file is absent or carries no linguist attrs.
    pub fn load(root: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(root.join(".gitattributes")).ok()?;
        Self::parse(root, &content)
    }

    fn parse(root: &Path, content: &str) -> Option<Self> {
        let mut overrides = Vec::new();
        let mut vendored = Vec::new();
        let mut generated = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let Some(matcher) = compile_pattern(pattern) else {
                continue;
            };

            for attr in parts {
                if let Some(lang) = attr.strip_prefix("linguist-language=") {
                    overrides.push((matcher.clone(), lang.to_string()));
                } else if let Some(rule) = parse_flag(attr, "linguist-vendored", &matcher) {
                    vendored.push(rule);
                } else if let Some(rule) = parse_flag(attr, "linguist-generated", &matcher) {
                    generated.push(rule);
                }
            }
        }

        if overrides.is_empty() && vendored.is_empty() && generated.is_empty() {
            return None;
        }
        Some(Self {
            root: root.to_path_buf(),
            overrides,
            vendored,
            generated,
        })
    }

    /// Language override for this path, if any (last matching rule wins).
    pub fn language_override(&self, path: &Path) -> Option<&str> {
        let rel = self.rel(path);
        self.overrides
            .iter()
            .rev()
            .find(|(matcher, _)| matcher.is_match(&rel))
            .map(|(_, lang)| lang.as_str())
    }

    /// Is this path excluded as vendored or generated?
    pub fn is_excluded(&self, path: &Path) -> bool {
        let rel = self.rel(path);
        last_state(&self.vendored, &rel) || last_state(&self.generated, &rel)
    }

    fn rel(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root)
            .or_else(|_| path.strip_prefix("./"))
            .unwrap_or(path)
            .to_path_buf()
    }
}

/// Final on/off state of an attribute for a path (last match wins).
fn last_state(rules: &[AttrRule], rel: &Path) -> bool {
    rules
        .iter()
        .rev()
        .find(|rule| rule.matcher.is_match(rel))
        .is_some_and(|rule| rule.enabled)
}

/// Parse `linguist-x`, `-linguist-x`, `linguist-x=true/false` forms.
fn parse_flag(attr: &str, name: &str, matcher: &GlobMatcher) -> Option<AttrRule> {
    let (enabled, rest) = match attr.strip_prefix('-') {
        Some(rest) => (false, rest),
        None => (true, attr),
    };
    if rest == name {
        return Some(AttrRule {
            matcher: matcher.clone(),
            enabled,
        });
    }
    if let Some(value) = rest.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
        return Some(AttrRule {
            matcher: matcher.clone(),
            enabled: enabled && value != "false",
        });
    }
    None
}

/// Translate a .gitattributes pattern into a glob matcher.
fn compile_pattern(pattern: &str) -> Option<GlobMatcher> {
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let full = if pattern.contains('/') {
        pattern.to_string()
    } else {
        // No slash: matches at any depth, like .gitignore.
        format!("**/{pattern}")
    };
    Glob::new(&full).ok().map(|g| g.compile_matcher())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(content: &str) -> LinguistAttributes {
        LinguistAttributes::parse(Path::new("."), content).unwrap()
    }

    #[test]
    fn test_language_override() {
        let attrs = make("*.jsp linguist-language=HTML\ndocs/*.md linguist-language=Text\n");
        assert_eq!(
            attrs.language_override(Path::new("web/page.jsp")),
            Some("HTML")
        );
        assert_eq!(
            attrs.language_override(Path::new("docs/a.md")),
            Some("Text")
        );
        assert_eq!(attrs.language_override(Path::new("src/main.rs")), None);
    }

    #[test]
    fn test_last_override_wins() {
        let attrs = make("*.x linguist-language=A\nsub/*.x linguist-language=B\n");
        assert_eq!(attrs.language_override(Path::new("sub/f.x")), Some("B"));
        assert_eq!(attrs.language_override(Path::new("f.x")), Some("A"));
    }

    #[test]
    fn test_vendored_and_generated_exclude() {
        let attrs = make("vendor/** linguist-vendored\nsrc/gen/*.rs linguist-generated=true\n");
        assert!(attrs.is_excluded(Path::new("vendor/lib/x.js")));
        assert!(attrs.is_excluded(Path::new("src/gen/schema.rs")));
        assert!(!attrs.is_excluded(Path::new("src/main.rs")));
    }

    #[test]
    fn test_negated_reincludes() {
        let attrs =
            make("third_party/** linguist-vendored\nthird_party/ours/** -linguist-vendored\n");
        assert!(attrs.is_excluded(Path::new("third_party/dep/x.c")));
        assert!(!attrs.is_excluded(Path::new("third_party/ours/y.c")));
    }

    #[test]
    fn test_false_value_reincludes() {
        let attrs = make("gen/** linguist-generated\ngen/keep.rs linguist-generated=false\n");
        assert!(attrs.is_excluded(Path::new("gen/a.rs")));
        assert!(!attrs.is_excluded(Path::new("gen/keep.rs")));
    }

    #[test]
    fn test_dot_prefixed_paths_match() {
        let attrs = make("vendor/** linguist-vendored\n");
        assert!(attrs.is_excluded(Path::new("./vendor/x.js")));
    }

    #[test]
    fn test_no_linguist_attrs_returns_none() {
        assert!(LinguistAttributes::parse(Path::new("."), "*.rs diff=rust\n").is_none());
        assert!(LinguistAttributes::parse(Path::new("."), "").is_none());
    }
}
