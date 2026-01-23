//! Pattern-based filtering using glob and regex.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;

use crate::config::FilterConfig;
use crate::error::{Error, Result};

use super::Filter;

/// Filter based on glob and regex patterns.
pub struct PatternFilter {
    /// Glob patterns to exclude.
    exclude_globs: Option<GlobSet>,
    /// Glob patterns to include (overrides exclude).
    include_globs: Option<GlobSet>,
    /// Regex patterns to exclude files.
    exclude_file_regex: Vec<Regex>,
    /// Regex patterns to include files.
    include_file_regex: Vec<Regex>,
    /// Regex patterns to exclude directories.
    exclude_dir_regex: Vec<Regex>,
    /// Target languages (empty = all).
    target_languages: Vec<String>,
}

impl PatternFilter {
    /// Create a new pattern filter from configuration.
    pub fn new(config: &FilterConfig) -> Result<Self> {
        let exclude_globs = Self::build_glob_set(&config.excludes)?;
        let include_globs = Self::build_glob_set(&config.includes)?;

        let exclude_file_regex = Self::compile_patterns(&config.exclude_files)?;
        let include_file_regex = Self::compile_patterns(&config.include_files)?;
        let exclude_dir_regex = Self::compile_patterns(&config.exclude_dirs)?;

        Ok(Self {
            exclude_globs,
            include_globs,
            exclude_file_regex,
            include_file_regex,
            exclude_dir_regex,
            target_languages: config.languages.clone(),
        })
    }

    fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>> {
        if patterns.is_empty() {
            return Ok(None);
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern).map_err(|e| Error::InvalidGlob {
                pattern: pattern.clone(),
                source: e,
            })?;
            builder.add(glob);
        }

        Ok(Some(builder.build().map_err(|e| Error::InvalidGlob {
            pattern: patterns.join(", "),
            source: e,
        })?))
    }

    fn compile_patterns(patterns: &[String]) -> Result<Vec<Regex>> {
        patterns
            .iter()
            .map(|p| {
                Regex::new(p).map_err(|e| Error::InvalidRegex {
                    pattern: p.clone(),
                    source: e,
                })
            })
            .collect()
    }

    fn matches_any_regex(path: &Path, regexes: &[Regex]) -> bool {
        let path_str = path.to_string_lossy();
        regexes.iter().any(|re| re.is_match(&path_str))
    }
}

impl Filter for PatternFilter {
    fn should_include(&self, path: &Path, is_dir: bool) -> bool {
        let path_str = path.to_string_lossy();

        // Check include patterns first (they take precedence)
        if let Some(ref include) = self.include_globs {
            if include.is_match(path) {
                return true;
            }
        }

        if !is_dir && !self.include_file_regex.is_empty() {
            if Self::matches_any_regex(path, &self.include_file_regex) {
                return true;
            }
        }

        // Check exclude patterns
        if let Some(ref exclude) = self.exclude_globs {
            if exclude.is_match(path) {
                return false;
            }
        }

        if is_dir {
            if Self::matches_any_regex(path, &self.exclude_dir_regex) {
                return false;
            }
        } else {
            if Self::matches_any_regex(path, &self.exclude_file_regex) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclude_glob() {
        let config = FilterConfig {
            excludes: vec!["*.test.js".to_string()],
            ..Default::default()
        };

        let filter = PatternFilter::new(&config).unwrap();

        assert!(!filter.should_include(Path::new("app.test.js"), false));
        assert!(filter.should_include(Path::new("app.js"), false));
    }

    #[test]
    fn test_include_overrides_exclude() {
        let config = FilterConfig {
            excludes: vec!["*.js".to_string()],
            includes: vec!["important.js".to_string()],
            ..Default::default()
        };

        let filter = PatternFilter::new(&config).unwrap();

        assert!(filter.should_include(Path::new("important.js"), false));
        assert!(!filter.should_include(Path::new("other.js"), false));
    }

    #[test]
    fn test_exclude_dir_regex() {
        let config = FilterConfig {
            exclude_dirs: vec!["node_modules".to_string()],
            ..Default::default()
        };

        let filter = PatternFilter::new(&config).unwrap();

        assert!(!filter.should_include(Path::new("node_modules"), true));
        assert!(filter.should_include(Path::new("src"), true));
    }
}
