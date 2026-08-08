//! Language registry for detecting file languages.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use regex::Regex;

use crate::error::Result;

use super::definition::{keywords_pattern, Language};

/// Built-in language definitions (embedded at compile time).
const BUILTIN_LANGUAGES: &str = include_str!("../../languages.toml");

/// Registry of known programming languages.
pub struct LanguageRegistry {
    /// Extension to language mapping.
    by_extension: HashMap<String, Arc<Language>>,
    /// Filename to language mapping.
    by_filename: HashMap<String, Arc<Language>>,
    /// Language name to definition.
    by_name: HashMap<String, Arc<Language>>,
}

impl LanguageRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            by_extension: HashMap::new(),
            by_filename: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    /// Create a registry with built-in language definitions.
    pub fn with_builtin() -> Result<Self> {
        let mut registry = Self::empty();
        registry.load_toml(BUILTIN_LANGUAGES)?;
        // Both objectivec and matlab claim ".m". scc keeps both candidates
        // and disambiguates per file by content (see boyter/scc
        // processor/detector.go DetermineLanguage); codelens maps each
        // extension to exactly one language, so pin the owner that matches
        // scc's output on real Objective-C sources instead of letting
        // registration order decide.
        let objc = registry
            .get("objectivec")
            .expect("built-in objectivec definition");
        registry.by_extension.insert(".m".to_string(), objc);
        Ok(registry)
    }

    /// Load language definitions from TOML content.
    ///
    /// A definition whose id matches an already-registered language
    /// replaces that language wholesale — fields are not merged.
    pub fn load_toml(&mut self, content: &str) -> Result<()> {
        // BTreeMap keys iterate in sorted order, so when two definitions
        // claim the same extension the winner never depends on hash
        // randomness.
        let languages: BTreeMap<String, Language> = toml::from_str(content)?;
        self.register_all(languages, None);
        Ok(())
    }

    /// Load additional language definitions from a file.
    ///
    /// Unlike `load_toml`, user files are validated eagerly: a regex that
    /// fails to compile is a hard error here, because the lazy compilation
    /// path silently disables function/complexity analysis and the user
    /// gets no signal at all.
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path).map_err(|e| crate::error::Error::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;
        // Attach the path to parse errors — a bare "failed to parse
        // language definitions" is undiagnosable for user-supplied files.
        let languages: BTreeMap<String, Language> =
            toml::from_str(&content).map_err(|e| crate::error::Error::LanguageFileParse {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
        // Validate everything before registering anything, so a rejected
        // file never leaves the registry half-updated.
        for (id, lang) in &languages {
            validate_regexes(id, lang, path)?;
        }
        self.register_all(languages, Some(path));
        Ok(())
    }

    /// Register a batch of parsed definitions.
    ///
    /// `source` is the user file the definitions came from, if any:
    /// extension conflicts are only worth a warning when the user can act
    /// on them — the built-in table has a known overlap (".m") that
    /// `with_builtin` resolves explicitly.
    fn register_all(&mut self, languages: BTreeMap<String, Language>, source: Option<&Path>) {
        for (id, mut lang) in languages {
            // Use the key as name if not specified
            if lang.name.is_empty() || lang.name == "Unknown" {
                lang.name = id.clone();
            }

            // Same-id redefinition replaces the whole language: drop every
            // lookup entry still pointing at the old definition so detect()
            // and get() cannot disagree about which definition is live.
            if let Some(old) = self.by_name.get(&id.to_lowercase()).cloned() {
                self.by_extension.retain(|_, l| !Arc::ptr_eq(l, &old));
                self.by_filename.retain(|_, l| !Arc::ptr_eq(l, &old));
                self.by_name.retain(|_, l| !Arc::ptr_eq(l, &old));
            }

            let lang = Arc::new(lang);

            // Register by extension
            for ext in &lang.extensions {
                let ext = normalize_extension(ext);
                let displaced = self.by_extension.insert(ext.clone(), Arc::clone(&lang));
                if let (Some(path), Some(prev)) = (source, displaced) {
                    if !Arc::ptr_eq(&prev, &lang) {
                        eprintln!(
                            "warning: {}: extension '{}' moved from {} to {}",
                            path.display(),
                            ext,
                            prev.name,
                            lang.name
                        );
                    }
                }
            }

            // Register by filename
            for filename in &lang.filenames {
                self.by_filename
                    .insert(filename.to_lowercase(), Arc::clone(&lang));
            }

            // Register by name
            self.by_name.insert(lang.name.clone(), Arc::clone(&lang));
            self.by_name.insert(id.to_lowercase(), Arc::clone(&lang));
        }
    }

    /// Map an extra file extension onto an already-registered language
    /// (the `--count-as jsp:html` feature).
    pub fn map_extension(&mut self, ext: &str, lang_name: &str) -> Result<()> {
        let lang = self
            .get(lang_name)
            .ok_or_else(|| crate::error::Error::InvalidLanguage {
                name: lang_name.to_string(),
                reason: "unknown language in --count-as mapping (see --list-languages)".to_string(),
            })?;
        self.by_extension.insert(normalize_extension(ext), lang);
        Ok(())
    }

    /// Detect the language of a file by its path, falling back to the
    /// shebang line for extensionless scripts (`#!/usr/bin/env python`).
    pub fn detect_with_content(&self, path: &Path, content: &[u8]) -> Option<Arc<Language>> {
        if let Some(lang) = self.detect(path) {
            return Some(lang);
        }
        // Only fall back for files with no extension at all — an unknown
        // extension is a deliberate "not a source file" signal.
        if path.extension().is_some() {
            return None;
        }
        let interpreter = parse_shebang(content)?;
        self.get(interpreter_language_id(&interpreter))
    }

    /// Detect the language of a file by its path.
    pub fn detect(&self, path: &Path) -> Option<Arc<Language>> {
        // First, try to match by filename
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(lang) = self.by_filename.get(&filename.to_lowercase()) {
                return Some(Arc::clone(lang));
            }
        }

        // Then, try to match by extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = format!(".{}", ext.to_lowercase());
            if let Some(lang) = self.by_extension.get(&ext) {
                return Some(Arc::clone(lang));
            }
        }

        None
    }

    /// Get a language by name.
    pub fn get(&self, name: &str) -> Option<Arc<Language>> {
        self.by_name
            .get(name)
            .or_else(|| self.by_name.get(&name.to_lowercase()))
            .map(Arc::clone)
    }

    /// Get all registered languages, each exactly once.
    ///
    /// `by_name` usually holds two keys per language (display name and
    /// lowercase id), so deduplicate by allocation identity — name-based
    /// dedup would break when display name and id coincide.
    pub fn all(&self) -> impl Iterator<Item = &Arc<Language>> {
        let mut seen = HashSet::new();
        self.by_name
            .values()
            .filter(move |lang| seen.insert(Arc::as_ptr(lang)))
    }

    /// Get the number of registered languages.
    pub fn len(&self) -> usize {
        // Derived from all() so the two can never disagree.
        self.all().count()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_builtin().unwrap_or_else(|_| Self::empty())
    }
}

/// Normalize an extension to lowercase with a leading dot.
fn normalize_extension(ext: &str) -> String {
    if ext.starts_with('.') {
        ext.to_lowercase()
    } else {
        format!(".{}", ext.to_lowercase())
    }
}

/// Validate the regex fields of a user-supplied language definition.
///
/// Only called on the `load_file` path: built-in definitions rely on the
/// lazily-built complexity cache and must not pay for (or fail) regex
/// compilation at startup.
fn validate_regexes(id: &str, lang: &Language, path: &Path) -> Result<()> {
    if let Some(pattern) = &lang.function_pattern {
        if let Err(e) = Regex::new(pattern) {
            return Err(crate::error::Error::InvalidLanguage {
                name: id.to_string(),
                reason: format!("invalid function_pattern in {}: {e}", path.display()),
            });
        }
    }
    if let Some(pattern) = keywords_pattern(&lang.complexity_keywords) {
        if let Err(e) = Regex::new(&pattern) {
            return Err(crate::error::Error::InvalidLanguage {
                name: id.to_string(),
                reason: format!("invalid complexity_keywords in {}: {e}", path.display()),
            });
        }
    }
    Ok(())
}

/// Extract the interpreter name from a `#!` first line.
/// Handles the `/usr/bin/env <interp>` indirection and drops
/// trailing version digits (`python3` → `python`).
fn parse_shebang(content: &[u8]) -> Option<String> {
    let rest = content.strip_prefix(b"#!")?;
    let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
    let line = std::str::from_utf8(&rest[..line_end]).ok()?;

    let mut words = line.split_whitespace();
    let first = words.next()?;
    let mut interp = first.rsplit('/').next().unwrap_or(first);
    if interp == "env" {
        // Skip env options like -S; the interpreter is the first non-flag word
        interp = words.find(|w| !w.starts_with('-'))?;
        interp = interp.rsplit('/').next().unwrap_or(interp);
    }
    Some(
        interp
            .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
            .to_string(),
    )
}

/// Map an interpreter name to a registry language id.
fn interpreter_language_id(interpreter: &str) -> &str {
    match interpreter {
        "sh" | "dash" | "ksh" => "bash",
        "node" | "nodejs" | "deno" | "bun" => "javascript",
        "Rscript" => "r",
        other => other, // python, ruby, perl, php, lua, bash, zsh, fish, ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_by_extension() {
        let registry = LanguageRegistry::with_builtin().unwrap();

        let path = Path::new("main.rs");
        let lang = registry.detect(path);
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().name, "Rust");
    }

    #[test]
    fn test_detect_by_filename() {
        let registry = LanguageRegistry::with_builtin().unwrap();

        let path = Path::new("Makefile");
        let lang = registry.detect(path);
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().name, "Makefile");
    }

    #[test]
    fn test_unknown_extension() {
        let registry = LanguageRegistry::with_builtin().unwrap();

        let path = Path::new("file.unknown_extension_xyz");
        let lang = registry.detect(path);
        assert!(lang.is_none());
    }

    #[test]
    fn test_shebang_detection() {
        let registry = LanguageRegistry::with_builtin().unwrap();

        let cases: &[(&[u8], &str)] = &[
            (b"#!/usr/bin/env python\nprint(1)\n", "Python"),
            (b"#!/usr/bin/python3\nprint(1)\n", "Python"),
            (b"#!/bin/bash\necho hi\n", "Bash"),
            (b"#!/bin/sh\necho hi\n", "Bash"),
            (b"#!/usr/bin/env -S node --harmony\n1\n", "JavaScript"),
        ];
        for (content, expected) in cases {
            let lang = registry
                .detect_with_content(Path::new("deploy"), content)
                .unwrap_or_else(|| {
                    panic!("no language for {:?}", String::from_utf8_lossy(content))
                });
            assert_eq!(&lang.name, expected);
        }

        // Unknown extension must NOT fall back to shebang
        assert!(registry
            .detect_with_content(Path::new("data.xyz"), b"#!/bin/bash\n")
            .is_none());
        // No shebang, no extension -> None
        assert!(registry
            .detect_with_content(Path::new("README"), b"hello\n")
            .is_none());
    }

    #[test]
    fn test_map_extension_count_as() {
        let mut registry = LanguageRegistry::with_builtin().unwrap();
        registry.map_extension("jsp", "html").unwrap();

        let lang = registry.detect(Path::new("page.jsp")).unwrap();
        assert_eq!(lang.name, "HTML");

        // Unknown language must be a hard error, not a silent no-op
        assert!(registry.map_extension("x", "no-such-language").is_err());
    }

    #[test]
    fn test_m_extension_owner_is_deterministic() {
        // Both objectivec and matlab claim ".m" in languages.toml. scc keeps
        // both and resolves per file by content, with real Objective-C files
        // almost always matching its heuristics; codelens maps each extension
        // to exactly one language, so ".m" is pinned to Objective-C. Build
        // many registries so hash-order luck cannot mask a regression.
        for _ in 0..20 {
            let registry = LanguageRegistry::with_builtin().unwrap();
            let lang = registry.detect(Path::new("controller.m")).unwrap();
            assert_eq!(lang.name, "Objective-C");
        }
    }

    #[test]
    fn test_override_builtin_replaces_whole_definition() {
        let mut registry = LanguageRegistry::with_builtin().unwrap();

        let toml = r#"
            [rust]
            name = "Rust"
            extensions = [".rs2"]
        "#;
        registry.load_toml(toml).unwrap();

        // The new extension resolves to the replacement definition, which
        // has no comment rules (replacement, not field merging).
        let lang = registry.detect(Path::new("main.rs2")).unwrap();
        assert_eq!(lang.name, "Rust");
        assert!(lang.line_comments.is_empty());

        // The old extension mapping must not survive the override, and
        // get() must agree with detect() about which definition is live.
        assert!(registry.detect(Path::new("main.rs")).is_none());
        let by_name = registry.get("rust").unwrap();
        assert!(Arc::ptr_eq(&by_name, &lang));
    }

    #[test]
    fn test_override_builtin_removes_stale_display_name() {
        let mut registry = LanguageRegistry::with_builtin().unwrap();
        let before = registry.len();

        let toml = r#"
            [rust]
            name = "Rust 2024"
            extensions = [".rs"]
        "#;
        registry.load_toml(toml).unwrap();

        // The stale "Rust" display-name key must not shadow the override.
        assert_eq!(registry.get("Rust").unwrap().name, "Rust 2024");
        assert_eq!(registry.get("rust").unwrap().name, "Rust 2024");
        // Replacing a language must not change the language count.
        assert_eq!(registry.len(), before);
    }

    #[test]
    fn test_load_file_rejects_invalid_function_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("langs.toml");
        std::fs::write(
            &path,
            r#"
                [broken]
                name = "Broken"
                extensions = [".brk"]
                function_pattern = "(unclosed"
            "#,
        )
        .unwrap();

        let mut registry = LanguageRegistry::with_builtin().unwrap();
        let err = registry.load_file(&path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("broken"),
            "error should name the language: {msg}"
        );
        assert!(
            msg.contains("langs.toml"),
            "error should include the file path: {msg}"
        );
        // Nothing from the invalid file may have been registered.
        assert!(registry.detect(Path::new("x.brk")).is_none());
    }

    #[test]
    fn test_load_toml_does_not_validate_regexes() {
        // Regex validation is a load_file (user file) concern only: the
        // built-in loading path must stay lazy so startup never compiles
        // all 66 languages' patterns.
        let mut registry = LanguageRegistry::empty();
        let toml = r#"
            [x]
            name = "X"
            function_pattern = "(unclosed"
        "#;
        registry.load_toml(toml).unwrap();
    }

    #[test]
    fn test_all_and_len_agree_without_duplicates() {
        let registry = LanguageRegistry::with_builtin().unwrap();

        let names: Vec<&str> = registry.all().map(|l| l.name.as_str()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len(), "all() must not repeat languages");
        assert_eq!(registry.len(), names.len(), "len() must match all()");

        // One language per table in the built-in TOML.
        let tables: std::collections::HashMap<String, toml::Value> =
            toml::from_str(BUILTIN_LANGUAGES).unwrap();
        assert_eq!(registry.len(), tables.len());
    }

    #[test]
    fn test_load_custom_language() {
        let mut registry = LanguageRegistry::empty();

        let toml = r#"
            [mylang]
            name = "MyLang"
            extensions = [".ml"]
            line_comments = [";;"]
        "#;

        registry.load_toml(toml).unwrap();

        let path = Path::new("test.ml");
        let lang = registry.detect(path);
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().name, "MyLang");
    }
}
