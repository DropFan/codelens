//! Git repository integration via CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::error::{Error, Result};

/// Start-of-header sentinel in `git log` output (`%x01`).
/// Header lines never collide with numstat records (`added\tdeleted\tpath`).
const HEADER_MARK: char = '\u{01}';
/// Field separator within a header line (`%x1f`).
const FIELD_SEP: char = '\u{1f}';

/// Git repository client using system git CLI.
pub struct GitClient {
    repo_path: PathBuf,
}

/// A single file's change within one commit.
#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub added: usize,
    pub deleted: usize,
}

/// One commit with metadata and per-file changes.
///
/// `files` is empty for merge commits (plain `git log --numstat` emits no
/// records for them). Paths are normalized to each file's final name when
/// the history contains renames.
#[derive(Debug, Clone, Serialize)]
pub struct CommitRecord {
    pub hash: String,
    /// Author name (`%aN`, folded through .mailmap when present).
    pub author: String,
    /// Committer date as unix epoch (`%ct`), consistent with how
    /// `git log --since` filters commits.
    pub timestamp: i64,
    pub files: Vec<FileChange>,
}

/// File change frequency data.
#[derive(Debug, Clone, Serialize)]
pub struct FileChurn {
    pub path: PathBuf,
    pub commits: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    /// Unix epoch of the newest commit touching this file within the window.
    pub last_commit_ts: i64,
}

/// Repository metadata.
#[derive(Debug, Clone, Serialize)]
pub struct RepoInfo {
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
}

impl GitClient {
    /// Detect if the given path is inside a git repository.
    /// Returns a GitClient rooted at the repository root.
    pub fn detect(path: &Path) -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(path)
            .output()
            .map_err(|e| Error::GitError {
                message: format!("failed to execute git: {e}"),
            })?;

        if !output.status.success() {
            return Err(Error::NotGitRepo {
                path: path.to_path_buf(),
            });
        }

        let repo_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(Self {
            repo_path: PathBuf::from(repo_path),
        })
    }

    /// Get the repository root path.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Get repository metadata (branch, last commit).
    pub fn repo_info(&self) -> Result<RepoInfo> {
        let branch = self.run_git(&["rev-parse", "--abbrev-ref", "HEAD"]).ok();
        let commit = self.run_git(&["rev-parse", "--short", "HEAD"]).ok();
        let author = self.run_git(&["log", "-1", "--format=%an"]).ok();
        let date = self.run_git(&["log", "-1", "--format=%ai"]).ok();

        Ok(RepoInfo {
            branch,
            commit,
            author,
            date,
        })
    }

    /// Get per-commit change records (newest first) within the given time window.
    ///
    /// `since` is passed directly to `git log --since`, e.g. "90 days ago", "2025-01-01".
    pub fn commit_log(&self, since: &str) -> Result<Vec<CommitRecord>> {
        self.commit_log_range(Some(since))
    }

    /// Get each file's first-commit timestamp (unix epoch) across the full
    /// history. Rename-aware: a renamed file keeps the age of its original.
    ///
    /// Walks the entire history once, so this is noticeably slower than the
    /// windowed queries on repositories with very long histories.
    pub fn first_commit_times(&self) -> Result<HashMap<PathBuf, i64>> {
        let commits = self.commit_log_range(None)?;
        let mut first: HashMap<PathBuf, i64> = HashMap::new();
        for commit in &commits {
            for change in &commit.files {
                first
                    .entry(change.path.clone())
                    .and_modify(|ts| *ts = (*ts).min(commit.timestamp))
                    .or_insert(commit.timestamp);
            }
        }
        Ok(first)
    }

    fn commit_log_range(&self, since: Option<&str>) -> Result<Vec<CommitRecord>> {
        let mut args = vec![
            // core.quotepath defaults to true and C-quotes non-ASCII paths
            // ("\344\270\255...") in numstat output; those never match the
            // real UTF-8 paths from the analyzer, silently dropping the
            // files from churn/hotspot/coupling/age.
            "-c".to_string(),
            "core.quotepath=false".to_string(),
            "log".to_string(),
            "--numstat".to_string(),
            "--format=%x01%H%x1f%aN%x1f%ct".to_string(),
        ];
        if let Some(since) = since {
            args.push(format!("--since={since}"));
        }
        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::GitError {
                message: format!("failed to execute git log: {e}"),
            })?;

        if !output.status.success() {
            // An empty repository (no commits yet) causes git log to fail.
            // Treat this as an empty result rather than an error.
            if self.run_git(&["rev-parse", "HEAD"]).is_err() {
                return Ok(vec![]);
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::GitError {
                message: format!("git log failed: {stderr}"),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_log(&stdout))
    }

    /// Get file change frequency within the given time window.
    ///
    /// `since` is passed directly to `git log --since`, e.g. "90 days ago", "2025-01-01".
    pub fn file_churn(&self, since: &str) -> Result<Vec<FileChurn>> {
        Ok(churn_from_commits(&self.commit_log(since)?))
    }

    /// Get total commit count in the given time window.
    pub fn commit_count(&self, since: &str) -> Result<usize> {
        let output = self.run_git(&["rev-list", "--count", "HEAD", &format!("--since={since}")])?;
        Ok(output.parse::<usize>().unwrap_or(0))
    }

    /// Per-commit changed line ranges (new-file side) for one file within
    /// the time window: one entry per commit touching the file, each a list
    /// of (start_line, line_count) pairs. Deletion-only hunks report the
    /// anchor line with a count of 1 so the surrounding code still counts
    /// as touched.
    pub fn file_commit_hunks(&self, path: &Path, since: &str) -> Result<Vec<Vec<(usize, usize)>>> {
        let output = Command::new("git")
            .args([
                "-c",
                "core.quotepath=false",
                "log",
                "-p",
                "-U0",
                "--format=%x01%H",
                &format!("--since={since}"),
                "--",
                &path.display().to_string(),
            ])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::GitError {
                message: format!("failed to execute git log -p: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::GitError {
                message: format!("git log -p failed: {stderr}"),
            });
        }

        Ok(parse_hunk_ranges(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Check whether `reference` resolves to a commit in this repository.
    pub fn rev_exists(&self, reference: &str) -> bool {
        self.run_git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ])
        .is_ok()
    }

    /// Materialize `reference` as a detached temporary worktree so the old
    /// tree can be analyzed like a normal directory. The worktree is removed
    /// when the returned guard drops.
    pub fn temp_worktree(&self, reference: &str) -> Result<TempWorktree> {
        let dir = std::env::temp_dir().join(format!(
            "codelens-baseline-{}-{}",
            std::process::id(),
            reference.replace(['/', '\\', ':'], "_")
        ));
        // A stale directory from a crashed run would make `worktree add` fail.
        let _ = std::fs::remove_dir_all(&dir);

        self.run_git(&[
            "worktree",
            "add",
            "--detach",
            "--force",
            &dir.display().to_string(),
            reference,
        ])?;

        Ok(TempWorktree {
            repo_path: self.repo_path.clone(),
            path: dir,
        })
    }

    fn run_git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::GitError {
                message: format!("failed to execute git: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::GitError {
                message: stderr.trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// A temporary detached git worktree, removed on drop.
pub struct TempWorktree {
    repo_path: PathBuf,
    path: PathBuf,
}

impl TempWorktree {
    /// Root directory of the materialized tree.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWorktree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                &self.path.display().to_string(),
            ])
            .current_dir(&self.repo_path)
            .output();
        // Backstop: clear leftover metadata if removal was interrupted.
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo_path)
            .output();
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Parse a numstat rename path (`old => new` or `pre{old => new}post`)
/// into the full (old, new) path pair.
fn parse_rename(raw: &str) -> Option<(PathBuf, PathBuf)> {
    if !raw.contains(" => ") {
        return None;
    }

    let join = |s: String| -> PathBuf {
        // Collapse artifacts from empty rename sides: "a//b" or leading "/"
        PathBuf::from(s.replace("//", "/").trim_start_matches('/'))
    };

    if let (Some(open), Some(close)) = (raw.find('{'), raw.find('}')) {
        if open < close {
            let prefix = &raw[..open];
            let suffix = &raw[close + 1..];
            let inner = &raw[open + 1..close];
            if let Some((old_mid, new_mid)) = inner.split_once(" => ") {
                let old = join(format!("{prefix}{old_mid}{suffix}"));
                let new = join(format!("{prefix}{new_mid}{suffix}"));
                return Some((old, new));
            }
        }
    }

    raw.split_once(" => ")
        .map(|(old, new)| (PathBuf::from(old), PathBuf::from(new)))
}

/// Parse `git log --numstat --format=%x01%H%x1f%aN%x1f%ct` output into
/// per-commit records (newest first, matching git log order).
///
/// Rename entries (`old => new`) are followed so that a renamed file's
/// full history appears under its current name. Relies on git log listing
/// commits newest-first, so a rename is seen before the renamed file's
/// older entries.
fn parse_log(output: &str) -> Vec<CommitRecord> {
    let mut final_name: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut commits: Vec<CommitRecord> = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(header) = line.strip_prefix(HEADER_MARK) {
            let mut fields = header.splitn(3, FIELD_SEP);
            let hash = fields.next().unwrap_or("").to_string();
            let author = fields.next().unwrap_or("").to_string();
            let timestamp = fields
                .next()
                .and_then(|t| t.parse::<i64>().ok())
                .unwrap_or(0);
            commits.push(CommitRecord {
                hash,
                author,
                timestamp,
                files: Vec::new(),
            });
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            continue;
        }
        // Numstat before any header would indicate malformed input; skip it.
        let Some(commit) = commits.last_mut() else {
            continue;
        };

        let added = parts[0].parse::<usize>().unwrap_or(0);
        let deleted = parts[1].parse::<usize>().unwrap_or(0);

        let path = if let Some((old, new)) = parse_rename(parts[2]) {
            // Chained renames resolve to the newest name because newer
            // commits (and their rename entries) were processed first.
            let target = final_name.get(&new).cloned().unwrap_or(new);
            final_name.insert(old, target.clone());
            target
        } else {
            // Resolve through renames seen SO FAR (i.e. in newer commits
            // only). This keeps the mapping time-aware: when a file is
            // renamed away and a new file is later created under the old
            // name, the new file's commits keep their own name instead of
            // being folded into the rename target.
            let raw = PathBuf::from(parts[2]);
            final_name.get(&raw).cloned().unwrap_or(raw)
        };

        commit.files.push(FileChange {
            path,
            added,
            deleted,
        });
    }

    commits
}

/// Parse `git log -p -U0 --format=%x01%H` output into per-commit changed
/// line ranges on the new-file side. Hunk headers look like
/// `@@ -a,b +c,d @@` (`,b`/`,d` omitted when 1).
fn parse_hunk_ranges(output: &str) -> Vec<Vec<(usize, usize)>> {
    let mut commits: Vec<Vec<(usize, usize)>> = Vec::new();
    for line in output.lines() {
        if line.starts_with(HEADER_MARK) {
            commits.push(Vec::new());
            continue;
        }
        let Some(rest) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some(commit) = commits.last_mut() else {
            continue;
        };
        // Take the "+c,d" part.
        let Some(plus) = rest.split(' ').find(|part| part.starts_with('+')) else {
            continue;
        };
        let mut nums = plus[1..].splitn(2, ',');
        let Some(start) = nums.next().and_then(|n| n.parse::<usize>().ok()) else {
            continue;
        };
        let count: usize = nums.next().and_then(|n| n.parse().ok()).unwrap_or(1);
        // Deletion-only hunks (count 0, and start 0 when the deletion is at
        // the top of the file) anchor to the nearest surviving line.
        commit.push((start.max(1), count.max(1)));
    }
    commits.retain(|c| !c.is_empty());
    commits
}

/// Per-file author knowledge data (code-maat style ownership).
#[derive(Debug, Clone, Serialize)]
pub struct FileAuthors {
    /// Distinct authors touching the file within the window.
    pub authors: usize,
    /// Author with the largest contribution (added lines, commits as
    /// tie-breaker).
    pub main_author: String,
    /// Main author's share of the total contribution, 0.0-1.0.
    pub ownership: f64,
}

/// Aggregate per-commit records into per-file author ownership.
///
/// Contribution is weighted by added lines plus one point per commit, so
/// touch-only commits (renames, deletions) still count toward knowledge.
pub fn aggregate_authors(commits: &[CommitRecord]) -> HashMap<PathBuf, FileAuthors> {
    // path → author → contribution score
    let mut per_file: HashMap<&Path, HashMap<&str, usize>> = HashMap::new();
    for commit in commits {
        // A commit touches each path once, whatever normalization did.
        let mut seen: HashMap<&Path, usize> = HashMap::new();
        for change in &commit.files {
            *seen.entry(change.path.as_path()).or_insert(0) += change.added;
        }
        for (path, added) in seen {
            *per_file
                .entry(path)
                .or_default()
                .entry(commit.author.as_str())
                .or_insert(0) += added + 1;
        }
    }

    per_file
        .into_iter()
        .map(|(path, by_author)| {
            let total: usize = by_author.values().sum();
            let (main_author, main_score) = by_author
                .iter()
                .max_by_key(|(author, score)| (**score, std::cmp::Reverse(*author)))
                .map(|(a, s)| ((*a).to_string(), *s))
                .unwrap_or_default();
            let ownership = if total > 0 {
                main_score as f64 / total as f64
            } else {
                0.0
            };
            (
                path.to_path_buf(),
                FileAuthors {
                    authors: by_author.len(),
                    main_author,
                    ownership,
                },
            )
        })
        .collect()
}

/// Aggregate per-commit records into per-file churn data,
/// sorted by commit count (descending).
pub fn churn_from_commits(commits: &[CommitRecord]) -> Vec<FileChurn> {
    let mut file_map: HashMap<PathBuf, (usize, usize, usize, i64)> = HashMap::new();
    for commit in commits {
        // Rename normalization can leave one commit with several entries
        // for the same final path; merge them first so the commit counts
        // once per file.
        let mut per_commit: HashMap<&PathBuf, (usize, usize)> = HashMap::new();
        for change in &commit.files {
            let entry = per_commit.entry(&change.path).or_insert((0, 0));
            entry.0 += change.added;
            entry.1 += change.deleted;
        }
        for (path, (added, deleted)) in per_commit {
            let entry = file_map.entry(path.clone()).or_insert((0, 0, 0, 0));
            entry.0 += 1;
            entry.1 += added;
            entry.2 += deleted;
            entry.3 = entry.3.max(commit.timestamp);
        }
    }

    let mut churns: Vec<FileChurn> = file_map
        .into_iter()
        .map(|(path, (commits, added, deleted, last_ts))| FileChurn {
            path,
            commits,
            lines_added: added,
            lines_deleted: deleted,
            last_commit_ts: last_ts,
        })
        .collect();

    churns.sort_by_key(|c| std::cmp::Reverse(c.commits));
    churns
}

/// Parse a human-friendly duration string into a git --since compatible string.
///
/// Supported formats: "30d", "4w", "6m", "1y", "2025-01-01"
pub fn parse_since(input: &str) -> String {
    let input = input.trim();

    if input.len() == 10 && input.chars().nth(4) == Some('-') {
        return input.to_string();
    }

    if let Some(num_str) = input.strip_suffix('d') {
        if let Ok(n) = num_str.parse::<u32>() {
            return format!("{n} days ago");
        }
    }
    if let Some(num_str) = input.strip_suffix('w') {
        if let Ok(n) = num_str.parse::<u32>() {
            return format!("{} days ago", n * 7);
        }
    }
    if let Some(num_str) = input.strip_suffix('m') {
        if let Ok(n) = num_str.parse::<u32>() {
            return format!("{n} months ago");
        }
    }
    if let Some(num_str) = input.strip_suffix('y') {
        if let Ok(n) = num_str.parse::<u32>() {
            return format!("{n} years ago");
        }
    }

    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a header line in the `%x01%H%x1f%aN%x1f%ct` format.
    fn header(hash: &str, author: &str, ts: i64) -> String {
        format!("\u{01}{hash}\u{1f}{author}\u{1f}{ts}")
    }

    #[test]
    fn test_parse_log_empty() {
        assert!(parse_log("").is_empty());
    }

    #[test]
    fn test_parse_log_single_commit() {
        let input = format!(
            "{}\n5\t3\tsrc/main.rs\n2\t1\tsrc/lib.rs\n",
            header("abc1234", "Alice", 1_700_000_000)
        );
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].hash, "abc1234");
        assert_eq!(commits[0].author, "Alice");
        assert_eq!(commits[0].timestamp, 1_700_000_000);
        assert_eq!(commits[0].files.len(), 2);
        assert_eq!(commits[0].files[0].path, Path::new("src/main.rs"));
        assert_eq!(commits[0].files[0].added, 5);
        assert_eq!(commits[0].files[0].deleted, 3);
    }

    #[test]
    fn test_parse_log_merge_commit_has_no_files() {
        let input = format!(
            "{}\n{}\n5\t3\tsrc/main.rs\n",
            header("merge01", "Alice", 1_700_000_100),
            header("abc1234", "Bob", 1_700_000_000)
        );
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 2);
        assert!(commits[0].files.is_empty());
        assert_eq!(commits[1].files.len(), 1);
    }

    #[test]
    fn test_churn_single_commit() {
        let input = format!(
            "{}\n5\t3\tsrc/main.rs\n2\t1\tsrc/lib.rs\n",
            header("abc1234", "Alice", 1_700_000_000)
        );
        let result = churn_from_commits(&parse_log(&input));
        assert_eq!(result.len(), 2);
        let main = result
            .iter()
            .find(|f| f.path == Path::new("src/main.rs"))
            .unwrap();
        assert_eq!(main.commits, 1);
        assert_eq!(main.lines_added, 5);
        assert_eq!(main.lines_deleted, 3);
        assert_eq!(main.last_commit_ts, 1_700_000_000);
    }

    #[test]
    fn test_churn_multiple_commits_same_file() {
        // Newest first: last_commit_ts must come from the newest commit.
        let input = format!(
            "{}\n5\t3\tsrc/main.rs\n\n{}\n10\t2\tsrc/main.rs\n",
            header("abc1234", "Alice", 1_700_000_200),
            header("def5678", "Bob", 1_700_000_100)
        );
        let result = churn_from_commits(&parse_log(&input));
        assert_eq!(result.len(), 1);
        let main = &result[0];
        assert_eq!(main.commits, 2);
        assert_eq!(main.lines_added, 15);
        assert_eq!(main.lines_deleted, 5);
        assert_eq!(main.last_commit_ts, 1_700_000_200);
    }

    #[test]
    fn test_churn_binary_files() {
        let input = format!(
            "{}\n-\t-\timage.png\n5\t3\tsrc/main.rs\n",
            header("abc1234", "Alice", 1_700_000_000)
        );
        let result = churn_from_commits(&parse_log(&input));
        let png = result
            .iter()
            .find(|f| f.path == Path::new("image.png"))
            .unwrap();
        assert_eq!(png.commits, 1);
        assert_eq!(png.lines_added, 0);
    }

    #[test]
    fn test_parse_rename_brace_form() {
        let (old, new) = parse_rename("docs/{plans => design}/a.md").unwrap();
        assert_eq!(old, PathBuf::from("docs/plans/a.md"));
        assert_eq!(new, PathBuf::from("docs/design/a.md"));
    }

    #[test]
    fn test_parse_rename_brace_empty_side() {
        let (old, new) = parse_rename("{ => sub}/a.md").unwrap();
        assert_eq!(old, PathBuf::from("a.md"));
        assert_eq!(new, PathBuf::from("sub/a.md"));
    }

    #[test]
    fn test_parse_rename_whole_path() {
        let (old, new) = parse_rename("old.rs => new.rs").unwrap();
        assert_eq!(old, PathBuf::from("old.rs"));
        assert_eq!(new, PathBuf::from("new.rs"));
    }

    #[test]
    fn test_parse_rename_not_a_rename() {
        assert!(parse_rename("src/main.rs").is_none());
    }

    #[test]
    fn test_rename_merges_history() {
        // newest-first: rename commit, then older history under the old name
        let input = format!(
            "{}\n3\t1\tsrc/new.rs\n\n{}\n0\t0\tsrc/{{old.rs => new.rs}}\n\n{}\n10\t2\tsrc/old.rs\n\n{}\n5\t0\tsrc/old.rs\n",
            header("aaa111", "Alice", 1_700_000_400),
            header("bbb222", "Alice", 1_700_000_300),
            header("ccc333", "Bob", 1_700_000_200),
            header("ddd444", "Bob", 1_700_000_100)
        );
        let result = churn_from_commits(&parse_log(&input));
        assert_eq!(result.len(), 1, "old and new names must merge: {result:?}");
        let f = &result[0];
        assert_eq!(f.path, PathBuf::from("src/new.rs"));
        assert_eq!(f.commits, 4);
        assert_eq!(f.lines_added, 18);
        assert_eq!(f.lines_deleted, 3);
        assert_eq!(f.last_commit_ts, 1_700_000_400);
    }

    #[test]
    fn test_chained_rename() {
        // b => c (newer), then a => b (older): everything lands on c
        let input = format!(
            "{}\n0\t0\tb.rs => c.rs\n\n{}\n0\t0\ta.rs => b.rs\n\n{}\n7\t1\ta.rs\n",
            header("aaa", "Alice", 1_700_000_300),
            header("bbb", "Alice", 1_700_000_200),
            header("ccc", "Alice", 1_700_000_100)
        );
        let result = churn_from_commits(&parse_log(&input));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("c.rs"));
        assert_eq!(result[0].commits, 3);
        assert_eq!(result[0].lines_added, 7);
    }

    #[test]
    fn test_rename_rewrites_commit_records() {
        // A commit under the old name must surface the final name in
        // CommitRecord.files too, not just in the churn aggregate.
        let input = format!(
            "{}\n0\t0\told.rs => new.rs\n\n{}\n7\t1\told.rs\n",
            header("aaa", "Alice", 1_700_000_200),
            header("bbb", "Alice", 1_700_000_100)
        );
        let commits = parse_log(&input);
        assert_eq!(commits[1].files[0].path, PathBuf::from("new.rs"));
    }

    #[test]
    fn test_recreated_file_keeps_own_history() {
        // a.rs was renamed to b.rs, then a NEW a.rs was created and
        // modified. The new file's commits must stay under a.rs, not be
        // folded into b.rs. Input is newest-first.
        let input = format!(
            "{}\n1\t0\ta.rs\n\n{}\n1\t0\ta.rs\n\n{}\n2\t0\ta.rs\n\n{}\n0\t0\ta.rs => b.rs\n\n{}\n5\t0\ta.rs\n",
            header("c5", "Alice", 500),
            header("c4", "Alice", 400),
            header("c3", "Alice", 300),
            header("c2", "Alice", 200),
            header("c1", "Alice", 100)
        );
        let churns = churn_from_commits(&parse_log(&input));
        let a = churns
            .iter()
            .find(|c| c.path == Path::new("a.rs"))
            .expect("recreated a.rs must exist: {churns:?}");
        assert_eq!(a.commits, 3, "the new a.rs owns exactly its 3 commits");
        assert_eq!(a.lines_added, 4);
        assert_eq!(a.last_commit_ts, 500);
        let b = churns
            .iter()
            .find(|c| c.path == Path::new("b.rs"))
            .expect("b.rs must carry the pre-rename history");
        assert_eq!(b.commits, 2, "rename commit + original history");
        assert_eq!(b.lines_added, 5);
    }

    #[test]
    fn test_same_commit_merged_paths_count_once() {
        // A historic commit touched both x.rs and y.rs; later x.rs was
        // renamed to y.rs. After normalization that commit has two entries
        // for y.rs — it must count as ONE commit with summed lines.
        let input = format!(
            "{}\n0\t0\tx.rs => y.rs\n\n{}\n3\t1\tx.rs\n2\t1\ty.rs\n",
            header("c2", "Alice", 200),
            header("c1", "Alice", 100)
        );
        let churns = churn_from_commits(&parse_log(&input));
        assert_eq!(churns.len(), 1);
        let y = &churns[0];
        assert_eq!(y.path, PathBuf::from("y.rs"));
        assert_eq!(y.commits, 2, "c1 must count once despite two entries");
        assert_eq!(y.lines_added, 5);
        assert_eq!(y.lines_deleted, 2);
    }

    #[test]
    fn test_non_ascii_filenames_survive() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());
        // Force the C-quoting default even if the user's config disables it.
        Command::new("git")
            .args(["config", "core.quotepath", "true"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        std::fs::write(temp.path().join("中文文件.rs"), "fn main() {}\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let client = GitClient::detect(temp.path()).unwrap();
        let churns = client.file_churn("90 days ago").unwrap();
        assert_eq!(churns.len(), 1);
        assert_eq!(
            churns[0].path,
            PathBuf::from("中文文件.rs"),
            "non-ASCII paths must come back as real UTF-8, not C-quoted"
        );
    }

    #[test]
    fn test_aggregate_authors_ownership() {
        // Alice adds 90 lines over 2 commits, Bob adds 8 over 1 commit.
        let input = format!(
            "{}\n50\t0\tsrc/a.rs\n\n{}\n8\t2\tsrc/a.rs\n\n{}\n40\t1\tsrc/a.rs\n1\t0\tsrc/b.rs\n",
            header("c3", "Alice", 300),
            header("c2", "Bob", 200),
            header("c1", "Alice", 100)
        );
        let authors = aggregate_authors(&parse_log(&input));
        let a = &authors[&PathBuf::from("src/a.rs")];
        assert_eq!(a.authors, 2);
        assert_eq!(a.main_author, "Alice");
        // Alice: 50+1 + 40+1 = 92; Bob: 8+1 = 9 → 92/101
        assert!((a.ownership - 92.0 / 101.0).abs() < 0.001);
        let b = &authors[&PathBuf::from("src/b.rs")];
        assert_eq!(b.authors, 1);
        assert!((b.ownership - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_aggregate_authors_touch_only_commits_count() {
        // Bob's rename-only commit (0 added) still registers knowledge.
        let input = format!(
            "{}\n0\t0\tsrc/a.rs\n\n{}\n10\t0\tsrc/a.rs\n",
            header("c2", "Bob", 200),
            header("c1", "Alice", 100)
        );
        let authors = aggregate_authors(&parse_log(&input));
        let a = &authors[&PathBuf::from("src/a.rs")];
        assert_eq!(a.authors, 2);
        assert_eq!(a.main_author, "Alice");
    }

    #[test]
    fn test_parse_hunk_ranges() {
        let input = format!(
            "{}\n@@ -10,3 +12,5 @@ fn foo()\n@@ -30 +40 @@\n\n{}\n@@ -1,2 +0,0 @@\n",
            header("aaa", "Alice", 1),
            header("bbb", "Bob", 2)
        );
        let ranges = parse_hunk_ranges(&input);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], vec![(12, 5), (40, 1)]);
        // Deletion-only hunk anchors to line 1 with count 1.
        assert_eq!(ranges[1], vec![(1, 1)]);
    }

    #[test]
    fn test_parse_hunk_ranges_skips_diff_noise() {
        let input = format!(
            "{}\ndiff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,1 +1,2 @@\n+added line with @@ inside\n",
            header("aaa", "Alice", 1)
        );
        let ranges = parse_hunk_ranges(&input);
        assert_eq!(ranges, vec![vec![(1, 2)]]);
    }

    #[test]
    fn test_parse_since_days() {
        assert_eq!(parse_since("30d"), "30 days ago");
        assert_eq!(parse_since("7d"), "7 days ago");
    }

    #[test]
    fn test_parse_since_weeks() {
        assert_eq!(parse_since("4w"), "28 days ago");
    }

    #[test]
    fn test_parse_since_months() {
        assert_eq!(parse_since("6m"), "6 months ago");
    }

    #[test]
    fn test_parse_since_years() {
        assert_eq!(parse_since("1y"), "1 years ago");
    }

    #[test]
    fn test_parse_since_date() {
        assert_eq!(parse_since("2025-01-01"), "2025-01-01");
    }

    #[test]
    fn test_parse_since_passthrough() {
        assert_eq!(parse_since("3 months ago"), "3 months ago");
    }

    #[test]
    fn test_detect_in_git_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let client = GitClient::detect(temp.path());
        assert!(client.is_ok());
    }

    #[test]
    fn test_detect_not_git_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = GitClient::detect(temp.path());
        assert!(result.is_err());
    }

    fn init_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_file_churn_empty_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());
        let client = GitClient::detect(temp.path()).unwrap();
        let churns = client.file_churn("90 days ago").unwrap();
        assert!(churns.is_empty());
    }

    #[test]
    fn test_file_churn_with_commits() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        std::fs::write(
            temp.path().join("hello.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "update"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let client = GitClient::detect(temp.path()).unwrap();
        let churns = client.file_churn("90 days ago").unwrap();
        assert_eq!(churns.len(), 1);
        assert_eq!(churns[0].path, PathBuf::from("hello.rs"));
        assert_eq!(churns[0].commits, 2);
        assert!(churns[0].last_commit_ts > 0);
    }

    #[test]
    fn test_commit_log_with_commits() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());

        std::fs::write(temp.path().join("a.rs"), "fn a() {}\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let client = GitClient::detect(temp.path()).unwrap();
        let commits = client.commit_log("90 days ago").unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].author, "Test");
        assert!(commits[0].timestamp > 0);
        assert!(!commits[0].hash.is_empty());
        assert_eq!(commits[0].files.len(), 1);
        assert_eq!(commits[0].files[0].path, PathBuf::from("a.rs"));
    }
}
