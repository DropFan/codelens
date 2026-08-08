//! Test-code detection by path convention.
//!
//! File-level heuristics only: a directory named after a test convention,
//! or a filename following one. In-file test blocks (Rust's
//! `#[cfg(test)]`, D's `unittest`) are not detected — those lines still
//! count as production code.

use std::path::{Component, Path};

/// Directories that mark everything under them as test code.
const TEST_DIRS: &[&str] = &["test", "tests", "__tests__", "spec", "specs", "testdata"];

/// Does this path follow a test-code naming convention?
pub fn is_test_path(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        for comp in parent.components() {
            if let Component::Normal(os) = comp {
                if let Some(s) = os.to_str() {
                    if TEST_DIRS.contains(&s.to_ascii_lowercase().as_str()) {
                        return true;
                    }
                }
            }
        }
    }

    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();

    // Python
    if lower == "conftest.py" || lower.starts_with("test_") {
        return true;
    }
    // Go / Rust / Python: foo_test.go; Ruby: foo_spec.rb
    let lower_stem = lower.split('.').next().unwrap_or("");
    if lower_stem.ends_with("_test") || lower_stem.ends_with("_spec") {
        return true;
    }
    // JS/TS: foo.spec.ts, foo.test.tsx
    if lower.contains(".spec.") || lower.contains(".test.") {
        return true;
    }
    // Java/C#/Kotlin: FooTest.java, FooTests.cs (case-sensitive CamelCase)
    let stem = name.split('.').next().unwrap_or("");
    if stem.len() > 5 && (stem.ends_with("Test") || stem.ends_with("Tests")) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(path: &str) -> bool {
        is_test_path(&PathBuf::from(path))
    }

    #[test]
    fn test_directory_conventions() {
        assert!(check("tests/integration.rs"));
        assert!(check("src/__tests__/app.js"));
        assert!(check("spec/models/user_spec.rb"));
        assert!(check("pkg/testdata/fixture.go"));
        assert!(check("java/src/test/java/FooTest.java"));
    }

    #[test]
    fn test_filename_conventions() {
        assert!(check("pkg/parser_test.go"));
        assert!(check("app/test_models.py"));
        assert!(check("app/conftest.py"));
        assert!(check("src/Button.spec.tsx"));
        assert!(check("src/util.test.js"));
        assert!(check("src/main/java/UserServiceTest.java"));
        assert!(check("Services/OrderTests.cs"));
    }

    #[test]
    fn test_production_code_not_flagged() {
        assert!(!check("src/main.rs"));
        assert!(
            !check("src/test.rs"),
            "a file simply named test.rs is ambiguous; stay conservative"
        );
        assert!(
            !check("src/contest/entry.rs"),
            "directory must match exactly"
        );
        assert!(!check("src/latest.py"));
        assert!(
            !check("src/Attest.java"),
            "suffix match needs a real prefix"
        );
        assert!(!check("docs/protest.md"));
    }
}
