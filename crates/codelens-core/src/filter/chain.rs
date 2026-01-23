//! Filter chain that combines multiple filters.

use std::path::Path;

use crate::config::Config;
use crate::error::Result;

use super::pattern::PatternFilter;
use super::smart::SmartExclude;
use super::Filter;

/// Chain of filters applied in sequence.
pub struct FilterChain {
    filters: Vec<Box<dyn Filter>>,
}

impl FilterChain {
    /// Create a new filter chain from configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let mut filters: Vec<Box<dyn Filter>> = Vec::new();

        // Add pattern filter if patterns are specified
        if config.filter.has_patterns() {
            filters.push(Box::new(PatternFilter::new(&config.filter)?));
        }

        // Add smart exclude filter if enabled
        if config.filter.smart_exclude {
            filters.push(Box::new(SmartExclude::new()));
        }

        Ok(Self { filters })
    }

    /// Create an empty filter chain.
    pub fn empty() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Add a filter to the chain.
    pub fn add<F: Filter + 'static>(&mut self, filter: F) {
        self.filters.push(Box::new(filter));
    }
}

impl Filter for FilterChain {
    fn should_include(&self, path: &Path, is_dir: bool) -> bool {
        // All filters must pass
        self.filters
            .iter()
            .all(|f| f.should_include(path, is_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowAll;
    impl Filter for AllowAll {
        fn should_include(&self, _path: &Path, _is_dir: bool) -> bool {
            true
        }
    }

    struct DenyAll;
    impl Filter for DenyAll {
        fn should_include(&self, _path: &Path, _is_dir: bool) -> bool {
            false
        }
    }

    #[test]
    fn test_empty_chain_allows_all() {
        let chain = FilterChain::empty();
        assert!(chain.should_include(Path::new("test.rs"), false));
    }

    #[test]
    fn test_chain_all_must_pass() {
        let mut chain = FilterChain::empty();
        chain.add(AllowAll);
        chain.add(DenyAll);
        assert!(!chain.should_include(Path::new("test.rs"), false));
    }
}
