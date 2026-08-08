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
    files: Mutex<Vec<(PathBuf, Vec<LineHash>)>>,
}

impl DuplicationSink {
    pub fn record(&self, path: PathBuf, hashes: Vec<LineHash>) {
        self.files.lock().unwrap().push((path, hashes));
    }

    /// Drain the sink: (ULOC, per-file duplicated line-instance counts).
    /// ULOC counts every distinct non-blank line (scc parity); an
    /// eligible line instance is duplicated when its content occurs 2+
    /// times across the whole tree (including repeats within one file).
    pub fn finish(&self) -> (usize, HashMap<PathBuf, usize>) {
        let mut files = std::mem::take(&mut *self.files.lock().unwrap());

        // Overlapping input roots (`codelens . src`) walk a file twice;
        // keep one record per path or every line would count duplicate.
        let mut seen = std::collections::HashSet::new();
        files.retain(|(path, _)| seen.insert(path.clone()));

        let mut counts: HashMap<u64, u32> = HashMap::new();
        for (_, hashes) in &files {
            for &(h, _) in hashes {
                *counts.entry(h).or_insert(0) += 1;
            }
        }
        let uloc = counts.len();

        let per_file = files
            .into_iter()
            .map(|(path, hashes)| {
                let dup = hashes
                    .iter()
                    .filter(|(h, eligible)| *eligible && counts.get(h).copied().unwrap_or(0) >= 2)
                    .count();
                (path, dup)
            })
            .collect();

        (uloc, per_file)
    }
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
        // Overlapping roots walk the same file again.
        sink.record(PathBuf::from("src/a.rs"), hashes);

        let (uloc, per_file) = sink.finish();
        assert_eq!(uloc, 1);
        assert_eq!(
            per_file[&PathBuf::from("src/a.rs")],
            0,
            "a twice-walked unique file must not become 100% duplicate"
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
