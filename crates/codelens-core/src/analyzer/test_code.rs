//! Test-code detection by path convention and configured inline blocks.
//!
//! Single source of truth for "is this file a test?" — used by the stats
//! summary (Test Files / Test-to-Code ratio) and by change coupling
//! (tests are excluded from pairing by default). Inline block detection is
//! used only by v2 health scoring; general file and line totals stay intact.
//!
//! The heuristics are deliberately conservative: only widespread,
//! unambiguous conventions are recognized, because misclassifying a
//! production file is worse than missing a test.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use crate::analyzer::stats::HealthRegions;
use crate::language::Language;

use super::complexity::{mask_non_code, matching_brace_end};

/// Thread-safe collection point for per-file health region projections.
///
/// Paths are keys so overlapping input roots cannot duplicate a projection.
#[derive(Default)]
pub(crate) struct InlineTestSink {
    regions: Mutex<BTreeMap<PathBuf, HealthRegions>>,
}

impl InlineTestSink {
    pub(crate) fn record(&self, path: PathBuf, regions: HealthRegions) {
        self.regions
            .lock()
            .unwrap()
            .insert(super::normalize_path(&path), regions);
    }

    pub(crate) fn finish(&self) -> BTreeMap<PathBuf, HealthRegions> {
        std::mem::take(&mut *self.regions.lock().unwrap())
    }
}

/// Directory names that mark everything beneath them as test code.
///
/// Matched against complete path segments only — `contest/` or `latest/`
/// must never hit — and ASCII case-insensitively so Swift's `Tests/`
/// layout is covered.
const TEST_DIR_SEGMENTS: &[&str] = &["tests", "test", "__tests__", "spec", "specs", "testdata"];

/// Extensions where an upper-case `Test`/`Tests` file-name suffix is a
/// reliable convention (JUnit, kotlin.test, xUnit/NUnit). Kept to this
/// short list so e.g. `LatestNews.js` is never misread as a test.
const SUFFIX_CONVENTION_EXTS: &[&str] = &["java", "kt", "cs"];

/// Heuristic check whether a path points at a test file.
///
/// Recognized conventions:
/// - a full directory segment named `tests`, `test`, `__tests__`, `spec`,
///   `specs`, or `testdata`
/// - `*_test.*` (Go, Rust integration tests), `test_*.py`, `conftest.py`
/// - `*.test.*` / `*.spec.*` (JS/TS), `*_spec.rb`
/// - `*Test.java` / `*Tests.cs` / `*Test.kt` (and the `Test`/`Tests`
///   suffix cross-product, but only for those three extensions)
pub fn is_test_path(path: &Path) -> bool {
    // Directory segments: the file name itself is checked separately.
    if let Some(parent) = path.parent() {
        for component in parent.components() {
            if let Component::Normal(segment) = component {
                if let Some(segment) = segment.to_str() {
                    if TEST_DIR_SEGMENTS
                        .iter()
                        .any(|dir| segment.eq_ignore_ascii_case(dir))
                    {
                        return true;
                    }
                }
            }
        }
    }

    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name == "conftest.py" {
        return true;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };

    // Case-sensitive stem checks keep `attest.rs`, `protest.go`, and
    // `Contest.java` out; extensions compare case-insensitively.
    if stem.ends_with("_test") || stem.ends_with(".test") || stem.ends_with(".spec") {
        return true;
    }
    if ext.eq_ignore_ascii_case("py") && stem.starts_with("test_") {
        return true;
    }
    if ext.eq_ignore_ascii_case("rb") && stem.ends_with("_spec") {
        return true;
    }
    if SUFFIX_CONVENTION_EXTS
        .iter()
        .any(|e| ext.eq_ignore_ascii_case(e))
        && (stem.ends_with("Test") || stem.ends_with("Tests"))
    {
        return true;
    }

    false
}

/// Locate inline test blocks using language-provided start patterns.
///
/// Patterns must include the block's opening `{`. Comments and strings are
/// masked first, so examples in documentation cannot be mistaken for tests.
pub(crate) fn inline_test_ranges(content: &str, language: &Language) -> Vec<Range<usize>> {
    if language.inline_test_regexes().is_empty() {
        return Vec::new();
    }

    let masked = mask_non_code(content, language);
    let bytes = masked.as_bytes();
    let mut ranges = Vec::new();
    for matcher in language.inline_test_regexes() {
        for matched in matcher.find_iter(&masked) {
            let Some(relative_open) = masked[matched.start()..matched.end()].rfind('{') else {
                continue;
            };
            let open = matched.start() + relative_open;
            let Some(close) = matching_brace_end(bytes, open) else {
                continue;
            };
            ranges.push(matched.start()..close + 1);
        }
    }

    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut() {
            if range.start <= previous.end {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

/// Split a source file into whole physical lines belonging to production and
/// inline tests. Each line is emitted once, preventing double counting.
pub(crate) fn split_inline_tests(content: &str, language: &Language) -> Option<(String, String)> {
    let ranges = inline_test_ranges(content, language);
    if ranges.is_empty() {
        return None;
    }

    let mut production = String::new();
    let mut tests = String::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let end = offset + line.len();
        let is_test = ranges
            .iter()
            .any(|range| range.start < end && offset < range.end);
        if is_test {
            tests.push_str(line);
        } else {
            production.push_str(line);
        }
        offset = end;
    }
    Some((production, tests))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageRegistry;

    fn hit(path: &str) -> bool {
        is_test_path(Path::new(path))
    }

    #[test]
    fn inline_test_sink_deduplicates_lexically_equivalent_paths() {
        let sink = InlineTestSink::default();
        sink.record(PathBuf::from("src/lib.rs"), HealthRegions::default());
        sink.record(PathBuf::from("./src/lib.rs"), HealthRegions::default());

        let regions = sink.finish();
        assert_eq!(regions.len(), 1);
        assert!(regions.contains_key(Path::new("src/lib.rs")));
    }

    #[test]
    fn rust_inline_tests_are_split_without_matching_strings_or_nested_tests_twice() {
        let registry = LanguageRegistry::with_builtin().unwrap();
        let rust = registry.get("Rust").unwrap();
        let source = r##"
fn production() { work(); }
const EXAMPLE: &str = "#[cfg(test)] mod fake { ignored(); }";

#[cfg(test)]
mod tests {
    #[test]
    fn works() { assert!(true); }
}
"##;

        let ranges = inline_test_ranges(source, &rust);
        assert_eq!(ranges.len(), 1);
        let (production, tests) = split_inline_tests(source, &rust).unwrap();
        assert!(production.contains("fn production"));
        assert!(production.contains("mod fake"));
        assert!(!production.contains("fn works"));
        assert!(tests.contains("#[cfg(test)]"));
        assert!(tests.contains("fn works"));
    }

    #[test]
    fn zig_inline_test_blocks_are_split() {
        let registry = LanguageRegistry::with_builtin().unwrap();
        let zig = registry.get("Zig").unwrap();
        let source = "pub fn production() void {}\ntest \"works\" { try run(); }\n";

        let (production, tests) = split_inline_tests(source, &zig).unwrap();
        assert!(production.contains("pub fn production"));
        assert!(!production.contains("try run"));
        assert!(tests.contains("test \"works\""));
    }

    #[test]
    fn test_directory_segments_match() {
        assert!(hit("tests/common.rs"));
        assert!(hit("src/test/java/com/example/Foo.java"));
        assert!(hit("packages/ui/__tests__/button.js"));
        assert!(hit("spec/models/user.rb"));
        assert!(hit("specs/models/user.rb"));
        assert!(hit("pkg/parser/testdata/golden.json"));
        // Case-insensitive segments cover Swift's Tests/ layout.
        assert!(hit("Tests/AppTests/AppFeature.swift"));
    }

    #[test]
    fn test_directory_segments_require_full_match() {
        assert!(!hit("contest/main.rs"));
        assert!(!hit("latest/mod.rs"));
        assert!(!hit("src/protester/rally.py"));
        // The file name is not a directory segment.
        assert!(!hit("src/testdata.rs"));
    }

    #[test]
    fn test_underscore_test_suffix() {
        assert!(hit("pkg/parser_test.go"));
        assert!(hit("src/lib_test.rs"));
        assert!(!hit("src/attest.rs"));
        assert!(!hit("pkg/protest.go"));
    }

    #[test]
    fn test_python_conventions() {
        assert!(hit("app/test_models.py"));
        assert!(hit("app/conftest.py"));
        assert!(!hit("app/testify.py"));
        assert!(!hit("app/contest.py"));
        // test_* is a Python convention; other extensions stay untouched.
        assert!(!hit("lib/test_helper.rb"));
    }

    #[test]
    fn test_js_ts_dot_conventions() {
        assert!(hit("src/button.test.tsx"));
        assert!(hit("src/Button.spec.tsx"));
        assert!(hit("src/api.spec.ts"));
        assert!(hit("src/util.test.js"));
        assert!(!hit("src/latest.js"));
        assert!(!hit("src/prospect.ts"));
        assert!(!hit("components/LatestNews.js"));
    }

    #[test]
    fn test_ruby_spec_suffix() {
        assert!(hit("app/models/user_spec.rb"));
        assert!(!hit("app/models/prospec.rb"));
    }

    #[test]
    fn test_uppercase_suffix_gated_by_extension() {
        assert!(hit("src/main/FooTest.java"));
        assert!(hit("src/main/java/UserServiceTest.java"));
        assert!(hit("Services/OrderTests.cs"));
        assert!(hit("src/WidgetTests.cs"));
        assert!(hit("app/ParserTest.kt"));
        // The suffix convention never applies outside java/kt/cs.
        assert!(!hit("components/NewsTest.js"));
        // Case-sensitive suffix: Contest ends in "test", not "Test".
        assert!(!hit("src/Contest.java"));
        assert!(!hit("src/Attest.java"), "suffix match needs a real prefix");
    }

    #[test]
    fn test_plain_sources_pass_through() {
        assert!(!hit("src/main.rs"));
        assert!(!hit("pkg/server.go"));
        assert!(!hit("README.md"));
        assert!(
            !hit("src/test.rs"),
            "a file simply named test.rs is ambiguous; stay conservative"
        );
        assert!(!hit("docs/protest.md"));
    }
}
