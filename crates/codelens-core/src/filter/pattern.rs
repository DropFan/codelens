//! Pattern-based filtering using glob and regex.

use std::path::{Component, Path, PathBuf};

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
            || Self::path_suffixes(path).any(|candidate| {
                let path_str = candidate.to_string_lossy();
                regexes.iter().any(|re| re.is_match(&path_str))
            })
    }

    /// Match both the walker-provided path and every component-aligned suffix.
    /// Walkers commonly yield absolute paths, while CLI patterns are normally
    /// project-relative (`node_modules`, `src/**`, `*.test.js`).
    fn matches_glob(globs: &GlobSet, path: &Path) -> bool {
        globs.is_match(path)
            || Self::path_suffixes(path).any(|candidate| globs.is_match(candidate))
    }

    fn path_suffixes(path: &Path) -> impl Iterator<Item = PathBuf> + '_ {
        let components: Vec<_> = path
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part),
                _ => None,
            })
            .collect();

        (0..components.len()).map(move |start| components[start..].iter().collect())
    }
}

impl Filter for PatternFilter {
    fn should_include(&self, path: &Path, is_dir: bool) -> bool {
        // Check include patterns first (they take precedence)
        let matches_include_glob = self
            .include_globs
            .as_ref()
            .is_some_and(|include| Self::matches_glob(include, path));
        let matches_include_regex = !is_dir
            && !self.include_file_regex.is_empty()
            && Self::matches_any_regex(path, &self.include_file_regex);
        if matches_include_glob || matches_include_regex {
            return true;
        }

        // Directories must remain traversable while an include allowlist is
        // active; descendants, rather than the directory name, may match it.
        // Non-matching files are rejected below.
        let has_includes = self.include_globs.is_some() || !self.include_file_regex.is_empty();
        if is_dir && has_includes {
            return true;
        }

        // Check exclude patterns
        if let Some(ref exclude) = self.exclude_globs {
            if Self::matches_glob(exclude, path) {
                return false;
            }
        }

        if is_dir {
            if Self::matches_any_regex(path, &self.exclude_dir_regex) {
                return false;
            }
        } else if Self::matches_any_regex(path, &self.exclude_file_regex) {
            return false;
        }

        !has_includes
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
    fn test_bare_exclude_glob_matches_nested_directory() {
        let config = FilterConfig {
            excludes: vec!["node_modules".to_string()],
            ..Default::default()
        };

        let filter = PatternFilter::new(&config).unwrap();

        assert!(!filter.should_include(Path::new("/tmp/project/node_modules"), true));
        assert!(filter.should_include(Path::new("/tmp/project/src"), true));
    }

    #[test]
    fn test_include_file_regex_is_allowlist() {
        let config = FilterConfig {
            include_files: vec![r"main\.rs$".to_string()],
            ..Default::default()
        };

        let filter = PatternFilter::new(&config).unwrap();

        assert!(filter.should_include(Path::new("/tmp/project"), true));
        assert!(filter.should_include(Path::new("/tmp/project/src/main.rs"), false));
        assert!(!filter.should_include(Path::new("/tmp/project/src/lib.rs"), false));
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
