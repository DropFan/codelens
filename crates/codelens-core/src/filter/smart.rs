//! Smart directory exclusion based on project type.

use std::path::Path;

use super::Filter;

/// Common directories to always exclude.
const ALWAYS_EXCLUDE: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    ".bzr",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".coverage",
    ".tox",
    ".nox",
    ".eggs",
    "*.egg-info",
    ".cache",
    ".parcel-cache",
    ".next",
    ".nuxt",
    ".output",
    ".vercel",
    ".netlify",
    ".turbo",
    "coverage",
    ".nyc_output",
    ".gradle",
    ".idea",
    ".vscode",
    ".vs",
    "*.xcodeproj",
    "*.xcworkspace",
    "DerivedData",
    "Pods",
];

/// Smart exclusion filter that detects project types.
pub struct SmartExclude {
    /// Directories to always exclude.
    always_exclude: Vec<&'static str>,
}

impl SmartExclude {
    /// Create a new smart exclude filter.
    pub fn new() -> Self {
        Self {
            always_exclude: ALWAYS_EXCLUDE.to_vec(),
        }
    }

    /// Check if a directory name should be excluded.
    fn is_excluded_dir(&self, name: &str) -> bool {
        // Check always-exclude list
        for pattern in &self.always_exclude {
            if let Some(suffix) = pattern.strip_prefix('*') {
                // Glob pattern (e.g., "*.egg-info")
                if name.ends_with(suffix) {
                    return true;
                }
            } else if name == *pattern {
                return true;
            }
        }

        // Common build/dependency directories
        // Note: "packages" is NOT excluded as it often contains source code
        // in monorepos (Python, Rust, JS workspaces)
        matches!(
            name,
            "node_modules"
                | "vendor"
                | "target"
                | "build"
                | "dist"
                | "out"
                | "bin"
                | "obj"
                | "bower_components"
                | "jspm_packages"
                | ".bundle"
                | "venv"
                | ".venv"
                | "virtualenv"
        )
    }
}

impl Default for SmartExclude {
    fn default() -> Self {
        Self::new()
    }
}

impl Filter for SmartExclude {
    fn should_include(&self, path: &Path, is_dir: bool) -> bool {
        if !is_dir {
            return true;
        }

        // Get the directory name
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => return true,
        };

        // Check if it's a hidden directory (starts with .)
        if name.starts_with('.') && self.always_exclude.contains(&name) {
            return false;
        }

        !self.is_excluded_dir(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclude_node_modules() {
        let filter = SmartExclude::new();
        assert!(!filter.should_include(Path::new("node_modules"), true));
    }

    #[test]
    fn test_exclude_git() {
        let filter = SmartExclude::new();
        assert!(!filter.should_include(Path::new(".git"), true));
    }

    #[test]
    fn test_exclude_egg_info() {
        let filter = SmartExclude::new();
        assert!(!filter.should_include(Path::new("mypackage.egg-info"), true));
    }

    #[test]
    fn test_include_src() {
        let filter = SmartExclude::new();
        assert!(filter.should_include(Path::new("src"), true));
    }

    #[test]
    fn test_include_files() {
        let filter = SmartExclude::new();
        assert!(filter.should_include(Path::new("main.rs"), false));
        assert!(filter.should_include(Path::new("node_modules.txt"), false));
    }
}
