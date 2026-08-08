//! Language registry for detecting file languages.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::Result;

use super::definition::Language;

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
        Ok(registry)
    }

    /// Load language definitions from TOML content.
    pub fn load_toml(&mut self, content: &str) -> Result<()> {
        let languages: HashMap<String, Language> = toml::from_str(content)?;

        for (id, mut lang) in languages {
            // Use the key as name if not specified
            if lang.name.is_empty() || lang.name == "Unknown" {
                lang.name = id.clone();
            }

            let lang = Arc::new(lang);

            // Register by extension
            for ext in &lang.extensions {
                let ext = if ext.starts_with('.') {
                    ext.to_lowercase()
                } else {
                    format!(".{}", ext.to_lowercase())
                };
                self.by_extension.insert(ext, Arc::clone(&lang));
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

        Ok(())
    }

    /// Load additional language definitions from a file.
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path).map_err(|e| crate::error::Error::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;
        // Attach the path to parse errors — a bare "failed to parse
        // language definitions" is undiagnosable for user-supplied files.
        self.load_toml(&content).map_err(|e| match e {
            crate::error::Error::LanguageParse(source) => crate::error::Error::LanguageFileParse {
                path: path.to_path_buf(),
                source: Box::new(source),
            },
            other => other,
        })
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
        let ext = if ext.starts_with('.') {
            ext.to_lowercase()
        } else {
            format!(".{}", ext.to_lowercase())
        };
        self.by_extension.insert(ext, lang);
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

    /// Get all registered languages.
    pub fn all(&self) -> impl Iterator<Item = &Arc<Language>> {
        self.by_name.values()
    }

    /// Get the number of registered languages.
    pub fn len(&self) -> usize {
        // Count unique languages (by name)
        self.by_name.len() / 2 // Each language is registered twice (by name and id)
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
