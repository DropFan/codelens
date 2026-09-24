//! Line-level duplication analysis (ULOC / DRYness).
//!
//! Every non-blank line (whitespace-trimmed) is hashed during the walk;
//! afterwards the distinct-hash count is the project's ULOC (scc's
//! `sort -u | wc -l` equivalent) and each file learns how many of its
//! line instances appear more than once across the tree. Structural
//! noise (brace-only lines, repeated imports) duplicates everywhere, so
//! scoring curves are calibrated against that baseline rather than
//! expecting zero.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// FNV-1a: fast, dependency-free hashing for short line slices.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Lines shorter than this (trimmed) never count as duplication: braces,
/// `#[test]`, closing tokens repeat structurally in every codebase and
/// would drown the copy-paste signal. They still count toward ULOC.
const MIN_DUP_LINE_LEN: usize = 8;

/// One non-blank line's hash plus whether it may count as duplication.
pub type LineHash = (u64, bool);

/// Per-line (hash, duplication-eligible) pairs for a file's non-blank
/// lines, whitespace-trimmed.
pub fn line_hashes(content: &[u8]) -> Vec<LineHash> {
    content
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            let trimmed = line.trim_ascii();
            (!trimmed.is_empty()).then(|| (fnv1a(trimmed), trimmed.len() >= MIN_DUP_LINE_LEN))
        })
        .collect()
}

/// Thread-safe sink collecting per-file line hashes during the parallel
/// walk; consumed once afterwards to produce global duplication data.
#[derive(Default)]
pub struct DuplicationSink {
    files: Mutex<Vec<FileLineHashes>>,
}

struct FileLineHashes {
    path: PathBuf,
    all: Vec<LineHash>,
    regions: Option<RegionLineHashes>,
}

struct RegionLineHashes {
    production: Vec<LineHash>,
    tests: Vec<LineHash>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RegionDuplicateLines {
    pub(crate) production: usize,
    pub(crate) tests: usize,
}

impl DuplicationSink {
    pub fn record(&self, path: PathBuf, hashes: Vec<LineHash>) {
        self.files.lock().unwrap().push(FileLineHashes {
            path: super::normalize_path(&path),
            all: hashes,
            regions: None,
        });
    }

    /// Record full-file hashes together with production/test partitions.
    /// Global occurrence counts still come only from `all`, so each physical
    /// line contributes exactly once.
    pub(crate) fn record_with_regions(
        &self,
        path: PathBuf,
        all: Vec<LineHash>,
        production: Vec<LineHash>,
        tests: Vec<LineHash>,
    ) {
        self.files.lock().unwrap().push(FileLineHashes {
            path: super::normalize_path(&path),
            all,
            regions: Some(RegionLineHashes { production, tests }),
        });
    }

    /// Drain the sink: (ULOC, per-file duplicated line-instance counts).
    /// ULOC counts every distinct non-blank line (scc parity); an
    /// eligible line instance is duplicated when its content occurs 2+
    /// times across the whole tree (including repeats within one file).
    pub fn finish(&self) -> (usize, HashMap<PathBuf, usize>) {
        let (uloc, per_file, _) = self.finish_with_regions();
        (uloc, per_file)
    }

    pub(crate) fn finish_with_regions(
        &self,
    ) -> (
        usize,
        HashMap<PathBuf, usize>,
        HashMap<PathBuf, RegionDuplicateLines>,
    ) {
        let mut files = std::mem::take(&mut *self.files.lock().unwrap());

        // Overlapping input roots (`codelens . src`) walk a file twice;
        // keep one record per path or every line would count duplicate.
        let mut seen = std::collections::HashSet::new();
        files.retain(|file| seen.insert(file.path.clone()));

        let mut counts: HashMap<u64, u32> = HashMap::new();
        for file in &files {
            for &(h, _) in &file.all {
                *counts.entry(h).or_insert(0) += 1;
            }
        }
        let uloc = counts.len();

        let mut per_file = HashMap::with_capacity(files.len());
        let mut per_region = HashMap::new();
        for file in files {
            per_file.insert(file.path.clone(), duplicated_lines(&file.all, &counts));
            if let Some(regions) = file.regions {
                per_region.insert(
                    file.path,
                    RegionDuplicateLines {
                        production: duplicated_lines(&regions.production, &counts),
                        tests: duplicated_lines(&regions.tests, &counts),
                    },
                );
            }
        }

        (uloc, per_file, per_region)
    }
}

fn duplicated_lines(hashes: &[LineHash], counts: &HashMap<u64, u32>) -> usize {
    hashes
        .iter()
        .filter(|(hash, eligible)| *eligible && counts.get(hash).copied().unwrap_or(0) >= 2)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_hashes_skip_blank_and_trim() {
        let content = b"fn a() { body(); }\n\n   \n  fn a() { body(); }  \r\n";
        let hashes = line_hashes(content);
        assert_eq!(hashes.len(), 2, "blank lines are skipped");
        assert_eq!(hashes[0], hashes[1], "trimmed identical lines hash equal");
        assert!(hashes[0].1, "long lines are duplication-eligible");
    }

    #[test]
    fn test_short_lines_ineligible_but_counted_in_uloc() {
        let hashes = line_hashes(b"}\n#[test]\n");
        assert_eq!(hashes.len(), 2);
        assert!(hashes.iter().all(|(_, eligible)| !eligible));
    }

    #[test]
    fn test_sink_uloc_and_per_file() {
        let sink = DuplicationSink::default();
        // a.rs: two unique lines + one shared with b.rs
        sink.record(
            PathBuf::from("a.rs"),
            line_hashes(b"let alpha = 1;\nlet beta = 22;\nshared_line();\n"),
        );
        // b.rs: the shared line + an internal repeat + a short brace pair
        sink.record(
            PathBuf::from("b.rs"),
            line_hashes(b"shared_line();\ngamma_line();\ngamma_line();\n}\n}\n"),
        );

        let (uloc, per_file) = sink.finish();
        // distinct: alpha, beta, shared, gamma, "}"
        assert_eq!(uloc, 5);
        assert_eq!(per_file[&PathBuf::from("a.rs")], 1, "only the shared line");
        assert_eq!(
            per_file[&PathBuf::from("b.rs")],
            3,
            "shared + both gamma instances; brace lines are ineligible"
        );
    }

    #[test]
    fn test_sink_dedupes_twice_walked_files() {
        let sink = DuplicationSink::default();
        let hashes = line_hashes(b"only_line_in_repo();\n");
        sink.record(PathBuf::from("src/a.rs"), hashes.clone());
        // Overlapping roots may spell the same file differently.
        sink.record(PathBuf::from("./src/a.rs"), hashes);

        let (uloc, per_file) = sink.finish();
        assert_eq!(uloc, 1);
        assert_eq!(
            per_file[&PathBuf::from("src/a.rs")],
            0,
            "a twice-walked unique file must not become 100% duplicate"
        );
    }

    #[test]
    fn test_sink_attributes_duplicates_to_inline_regions() {
        let sink = DuplicationSink::default();
        let production = line_hashes(b"production_shared_line();\nproduction_unique_line();\n");
        let tests = line_hashes(b"test_shared_line();\ntest_unique_line();\n");
        let mut all = production.clone();
        all.extend(tests.clone());
        sink.record_with_regions(PathBuf::from("src/lib.rs"), all, production, tests);
        sink.record(
            PathBuf::from("src/other.rs"),
            line_hashes(b"production_shared_line();\ntest_shared_line();\n"),
        );

        let (_, per_file, per_region) = sink.finish_with_regions();

        assert_eq!(per_file[&PathBuf::from("src/lib.rs")], 2);
        assert_eq!(
            per_region[&PathBuf::from("src/lib.rs")],
            RegionDuplicateLines {
                production: 1,
                tests: 1,
            }
        );
    }

    #[test]
    fn test_sink_empty() {
        let sink = DuplicationSink::default();
        let (uloc, per_file) = sink.finish();
        assert_eq!(uloc, 0);
        assert!(per_file.is_empty());
    }
}
