//! Change coupling analysis: files that keep changing together.
//!
//! Two files that repeatedly appear in the same commits share a hidden
//! dependency the module structure does not express. The degree formula
//! follows code-maat's logical coupling: shared commits divided by the
//! average of both files' commit counts.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git::CommitRecord;

#[derive(Debug, Clone, Serialize)]
pub struct CouplingPair {
    pub file_a: PathBuf,
    pub file_b: PathBuf,
    /// Commits touching both files.
    pub shared_commits: usize,
    /// Commits touching file_a (within the window).
    pub commits_a: usize,
    /// Commits touching file_b (within the window).
    pub commits_b: usize,
    /// shared / avg(commits_a, commits_b), as a percentage.
    pub degree: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CouplingReport {
    pub pairs: Vec<CouplingPair>,
    pub since: String,
    pub total_commits: usize,
    /// Commits excluded from pairing because they touched more files than
    /// `max_changeset` (bulk renames, formatting sweeps).
    pub skipped_large_commits: usize,
    /// Set when the report is focused on a single file (--for).
    pub focus: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CouplingOptions {
    /// Minimum shared commits for a pair to be reported.
    pub min_shared: usize,
    /// Minimum coupling degree (percent) for a pair to be reported.
    pub min_degree: f64,
    /// Commits touching more files than this are excluded from pairing.
    pub max_changeset: usize,
    /// Maximum number of pairs to report.
    pub top_n: usize,
    /// Only report pairs involving this file.
    pub focus: Option<PathBuf>,
}

impl Default for CouplingOptions {
    fn default() -> Self {
        Self {
            min_shared: 5,
            min_degree: 30.0,
            max_changeset: 30,
            top_n: 20,
            focus: None,
        }
    }
}

/// Compute change coupling from per-commit records.
///
/// `universe` restricts the analysis to a known file set (typically the
/// files present in the current tree after filters); pass None to consider
/// every path in history. Per-file commit counts include every commit,
/// while pairs are only mined from commits at or under `max_changeset`
/// files, matching code-maat's behavior.
pub fn analyze(
    commits: &[CommitRecord],
    universe: Option<&BTreeSet<PathBuf>>,
    since: &str,
    opts: &CouplingOptions,
) -> CouplingReport {
    let in_universe = |path: &PathBuf| -> bool { universe.is_none_or(|set| set.contains(path)) };

    // Per-commit unique file sets (rename normalization can duplicate paths).
    let mut commit_files: Vec<BTreeSet<PathBuf>> = Vec::with_capacity(commits.len());
    for commit in commits {
        let files: BTreeSet<PathBuf> = commit
            .files
            .iter()
            .map(|c| c.path.clone())
            .filter(in_universe)
            .collect();
        commit_files.push(files);
    }

    let mut per_file: HashMap<&Path, usize> = HashMap::new();
    for files in &commit_files {
        for f in files {
            *per_file.entry(f.as_path()).or_insert(0) += 1;
        }
    }

    let mut skipped_large = 0usize;
    let mut pair_counts: HashMap<(&Path, &Path), usize> = HashMap::new();
    for files in &commit_files {
        if files.len() > opts.max_changeset {
            skipped_large += 1;
            continue;
        }
        if files.len() < 2 {
            continue;
        }
        let list: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                // BTreeSet iteration keeps the pair key ordered.
                *pair_counts.entry((list[i], list[j])).or_insert(0) += 1;
            }
        }
    }

    let mut pairs: Vec<CouplingPair> = pair_counts
        .into_iter()
        .filter_map(|((a, b), shared)| {
            if shared < opts.min_shared {
                return None;
            }
            let commits_a = per_file.get(a).copied().unwrap_or(0);
            let commits_b = per_file.get(b).copied().unwrap_or(0);
            let avg = (commits_a + commits_b) as f64 / 2.0;
            if avg == 0.0 {
                return None;
            }
            let degree = shared as f64 / avg * 100.0;
            if degree < opts.min_degree {
                return None;
            }
            if let Some(focus) = &opts.focus {
                if a != focus.as_path() && b != focus.as_path() {
                    return None;
                }
            }
            Some(CouplingPair {
                file_a: a.to_path_buf(),
                file_b: b.to_path_buf(),
                shared_commits: shared,
                commits_a,
                commits_b,
                degree,
            })
        })
        .collect();

    pairs.sort_by(|x, y| {
        y.degree
            .partial_cmp(&x.degree)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(y.shared_commits.cmp(&x.shared_commits))
    });
    pairs.truncate(opts.top_n);

    CouplingReport {
        pairs,
        since: since.to_string(),
        total_commits: commits.len(),
        skipped_large_commits: skipped_large,
        focus: opts.focus.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::FileChange;

    fn commit(hash: &str, ts: i64, paths: &[&str]) -> CommitRecord {
        CommitRecord {
            hash: hash.to_string(),
            author: "Test".to_string(),
            timestamp: ts,
            files: paths
                .iter()
                .map(|p| FileChange {
                    path: PathBuf::from(p),
                    added: 1,
                    deleted: 0,
                })
                .collect(),
        }
    }

    fn make_commits() -> Vec<CommitRecord> {
        // a.rs + b.rs together in 5 commits, a.rs alone in 1 more.
        let mut commits: Vec<CommitRecord> = (0..5)
            .map(|i| commit(&format!("c{i}"), 1_700_000_000 + i, &["a.rs", "b.rs"]))
            .collect();
        commits.push(commit("solo", 1_700_000_100, &["a.rs"]));
        commits
    }

    #[test]
    fn test_basic_coupling() {
        let report = analyze(&make_commits(), None, "90d", &CouplingOptions::default());
        assert_eq!(report.pairs.len(), 1);
        let pair = &report.pairs[0];
        assert_eq!(pair.shared_commits, 5);
        assert_eq!(pair.commits_a, 6);
        assert_eq!(pair.commits_b, 5);
        // 5 / avg(6, 5) = 5 / 5.5 ≈ 90.9%
        assert!((pair.degree - 90.909).abs() < 0.01);
    }

    #[test]
    fn test_min_shared_filters() {
        let commits = vec![
            commit("c1", 1, &["a.rs", "b.rs"]),
            commit("c2", 2, &["a.rs", "b.rs"]),
        ];
        let report = analyze(&commits, None, "90d", &CouplingOptions::default());
        assert!(report.pairs.is_empty(), "2 shared < min_shared 5");
    }

    #[test]
    fn test_min_degree_filters() {
        // 5 shared commits but a.rs has 50 total: degree = 5/27.5 ≈ 18% < 30%
        let mut commits: Vec<CommitRecord> = (0..5)
            .map(|i| commit(&format!("c{i}"), i, &["a.rs", "b.rs"]))
            .collect();
        for i in 0..45 {
            commits.push(commit(&format!("s{i}"), 100 + i, &["a.rs"]));
        }
        let report = analyze(&commits, None, "90d", &CouplingOptions::default());
        assert!(report.pairs.is_empty());
    }

    #[test]
    fn test_large_changesets_skipped() {
        let paths: Vec<String> = (0..40).map(|i| format!("f{i}.rs")).collect();
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        let commits: Vec<CommitRecord> = (0..6)
            .map(|i| commit(&format!("c{i}"), i, &path_refs))
            .collect();
        let report = analyze(&commits, None, "90d", &CouplingOptions::default());
        assert!(report.pairs.is_empty());
        assert_eq!(report.skipped_large_commits, 6);
    }

    #[test]
    fn test_universe_restricts_files() {
        let universe: BTreeSet<PathBuf> = [PathBuf::from("a.rs")].into_iter().collect();
        let report = analyze(
            &make_commits(),
            Some(&universe),
            "90d",
            &CouplingOptions::default(),
        );
        assert!(report.pairs.is_empty(), "b.rs is outside the universe");
    }

    #[test]
    fn test_focus_filters_pairs() {
        let mut commits = make_commits();
        // c.rs + d.rs form a second strong pair.
        for i in 0..5 {
            commits.push(commit(&format!("x{i}"), 200 + i, &["c.rs", "d.rs"]));
        }
        let opts = CouplingOptions {
            focus: Some(PathBuf::from("c.rs")),
            ..CouplingOptions::default()
        };
        let report = analyze(&commits, None, "90d", &opts);
        assert_eq!(report.pairs.len(), 1);
        assert!(
            report.pairs[0].file_a == Path::new("c.rs")
                || report.pairs[0].file_b == Path::new("c.rs")
        );
    }

    #[test]
    fn test_top_n_truncates() {
        let mut commits = Vec::new();
        for p in 0..10 {
            for i in 0..5 {
                commits.push(commit(
                    &format!("p{p}i{i}"),
                    (p * 10 + i) as i64,
                    &[&format!("a{p}.rs"), &format!("b{p}.rs")],
                ));
            }
        }
        let opts = CouplingOptions {
            top_n: 3,
            ..CouplingOptions::default()
        };
        let report = analyze(&commits, None, "90d", &opts);
        assert_eq!(report.pairs.len(), 3);
    }

    #[test]
    fn test_duplicate_paths_in_commit_count_once() {
        // Rename normalization can leave the same final path twice in one
        // commit; it must not create a self-pair or double-count.
        let mut c = commit("c1", 1, &["a.rs", "a.rs", "b.rs"]);
        c.files.push(FileChange {
            path: PathBuf::from("a.rs"),
            added: 0,
            deleted: 0,
        });
        let commits: Vec<CommitRecord> = (0..5)
            .map(|i| {
                let mut cc = c.clone();
                cc.hash = format!("c{i}");
                cc
            })
            .collect();
        let report = analyze(&commits, None, "90d", &CouplingOptions::default());
        assert_eq!(report.pairs.len(), 1);
        assert_eq!(report.pairs[0].commits_a, 5);
        assert_eq!(report.pairs[0].shared_commits, 5);
        assert!((report.pairs[0].degree - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_empty_commits() {
        let report = analyze(&[], None, "90d", &CouplingOptions::default());
        assert!(report.pairs.is_empty());
        assert_eq!(report.total_commits, 0);
    }
}
