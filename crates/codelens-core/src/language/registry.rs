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
        self.load_toml(&content)
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
