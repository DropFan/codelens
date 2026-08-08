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
        Ok(aggregate_churn(&self.commit_log(since)?))
    }

    /// Get total commit count in the given time window.
    pub fn commit_count(&self, since: &str) -> Result<usize> {
        let output = self.run_git(&["rev-list", "--count", "HEAD", &format!("--since={since}")])?;
        Ok(output.parse::<usize>().unwrap_or(0))
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
            PathBuf::from(parts[2])
        };

        commit.files.push(FileChange {
            path,
            added,
            deleted,
        });
    }

    // Second pass: non-rename records were stored with their raw path,
    // which may be an old name whose rename entry appeared earlier
    // (i.e. in a newer commit). Rewrite them to the final name.
    if !final_name.is_empty() {
        for commit in &mut commits {
            for change in &mut commit.files {
                if let Some(target) = final_name.get(&change.path) {
                    change.path = target.clone();
                }
            }
        }
    }

    commits
}

/// Aggregate per-commit records into per-file churn data,
/// sorted by commit count (descending).
fn aggregate_churn(commits: &[CommitRecord]) -> Vec<FileChurn> {
    let mut file_map: HashMap<PathBuf, (usize, usize, usize, i64)> = HashMap::new();
    for commit in commits {
        for change in &commit.files {
            let entry = file_map.entry(change.path.clone()).or_insert((0, 0, 0, 0));
            entry.0 += 1;
            entry.1 += change.added;
            entry.2 += change.deleted;
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
        let result = aggregate_churn(&parse_log(&input));
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
        let result = aggregate_churn(&parse_log(&input));
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
        let result = aggregate_churn(&parse_log(&input));
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
        let result = aggregate_churn(&parse_log(&input));
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
        let result = aggregate_churn(&parse_log(&input));
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
