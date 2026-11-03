//! Supplemental measurements collected alongside full-file statistics.

use std::path::Path;

use crate::language::Language;

use super::complexity::ComplexityAnalyzer;
use super::counter;
use super::duplication::{self, DuplicationSink};
use super::stats::{HealthRegions, RegionStats};
use super::test_code::{self, InlineTestSink};

pub(crate) fn record_supplemental_metrics(
    path: &Path,
    content: &[u8],
    text: &str,
    language: &Language,
    complexity_analyzer: &ComplexityAnalyzer,
    inline_test_sink: Option<&InlineTestSink>,
    duplication_sink: Option<&DuplicationSink>,
) {
    // Path-level tests take precedence. For production-path files, split
    // only blocks that the language definition can locate and close
    // reliably; full-file metrics remain untouched for v1.
    let inline_parts = if test_code::is_test_path(path) {
        None
    } else {
        test_code::split_inline_tests(text, language)
    };
    if let (Some(sink), Some((production, tests))) = (inline_test_sink, inline_parts.as_ref()) {
        sink.record(
            path.to_path_buf(),
            HealthRegions {
                production: analyze_region(production, language, complexity_analyzer),
                tests: analyze_region(tests, language, complexity_analyzer),
            },
        );
    }

    let Some(sink) = duplication_sink else {
        return;
    };
    let mut hashes = duplication::line_hashes(content);
    let mut region_hashes = inline_parts.as_ref().map(|(production, tests)| {
        (
            duplication::line_hashes(production.as_bytes()),
            duplication::line_hashes(tests.as_bytes()),
        )
    });

    // Documents and data formats repeat structural lines by nature, not by
    // copy-paste. They count toward ULOC but not the duplication dimension.
    let patterns = language.complexity_patterns();
    if patterns.function_re.is_none() && patterns.keywords_re.is_none() {
        for hash in &mut hashes {
            hash.1 = false;
        }
        if let Some((production, tests)) = &mut region_hashes {
            for hash in production.iter_mut().chain(tests.iter_mut()) {
                hash.1 = false;
            }
        }
    }
    if let Some((production, tests)) = region_hashes {
        sink.record_with_regions(path.to_path_buf(), hashes, production, tests);
    } else {
        sink.record(path.to_path_buf(), hashes);
    }
}

fn analyze_region(
    content: &str,
    language: &Language,
    complexity_analyzer: &ComplexityAnalyzer,
) -> RegionStats {
    let bytes = content.as_bytes();
    let (trie, mask) = language.tokens();
    RegionStats {
        lines: counter::count_stats(bytes, trie, *mask),
        size: bytes.len() as u64,
        duplicate_lines: 0,
        complexity: complexity_analyzer.analyze(content, language),
    }
}
