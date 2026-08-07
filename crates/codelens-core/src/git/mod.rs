//! Git repository integration via CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::error::{Error, Result};

/// Git repository client using system git CLI.
pub struct GitClient {
    repo_path: PathBuf,
}

/// File change frequency data.
#[derive(Debug, Clone, Serialize)]
pub struct FileChurn {
    pub path: PathBuf,
    pub commits: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
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

    /// Get file change frequency within the given time window.
    ///
    /// `since` is passed directly to `git log --since`, e.g. "90 days ago", "2025-01-01".
    pub fn file_churn(&self, since: &str) -> Result<Vec<FileChurn>> {
        let output = Command::new("git")
            .args([
                "log",
                "--numstat",
                "--format=%H",
                &format!("--since={since}"),
            ])
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
        Ok(parse_numstat(&stdout))
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

/// Parse `git log --numstat` output into per-file churn data.
///
/// Rename entries (`old => new`) are followed so that a renamed file's
/// full history is aggregated under its current name. Relies on git log
/// listing commits newest-first, so a rename is seen before the renamed
/// file's older entries.
fn parse_numstat(output: &str) -> Vec<FileChurn> {
    let mut final_name: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut records: Vec<(usize, usize, PathBuf)> = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() == 3 {
            let added = parts[0].parse::<usize>().unwrap_or(0);
            let deleted = parts[1].parse::<usize>().unwrap_or(0);

            if let Some((old, new)) = parse_rename(parts[2]) {
                // Chained renames resolve to the newest name because newer
                // commits (and their rename entries) were processed first.
                let target = final_name.get(&new).cloned().unwrap_or(new);
                final_name.insert(old, target.clone());
                records.push((added, deleted, target));
            } else {
                records.push((added, deleted, PathBuf::from(parts[2])));
            }
        }
    }

    let mut file_map: HashMap<PathBuf, (usize, usize, usize)> = HashMap::new();
    for (added, deleted, path) in records {
        let key = final_name.get(&path).cloned().unwrap_or(path);
        let entry = file_map.entry(key).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += added;
        entry.2 += deleted;
    }

    let mut churns: Vec<FileChurn> = file_map
        .into_iter()
        .map(|(path, (commits, added, deleted))| FileChurn {
            path,
            commits,
            lines_added: added,
            lines_deleted: deleted,
        })
        .collect();

    churns.sort_by(|a, b| b.commits.cmp(&a.commits));
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

    #[test]
    fn test_parse_numstat_empty() {
        let result = parse_numstat("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_numstat_single_commit() {
        let input = "abc1234\n5\t3\tsrc/main.rs\n2\t1\tsrc/lib.rs\n";
        let result = parse_numstat(input);
        assert_eq!(result.len(), 2);
        let main = result
            .iter()
            .find(|f| f.path == Path::new("src/main.rs"))
            .unwrap();
        assert_eq!(main.commits, 1);
        assert_eq!(main.lines_added, 5);
        assert_eq!(main.lines_deleted, 3);
    }

    #[test]
    fn test_parse_numstat_multiple_commits_same_file() {
        let input = "abc1234\n5\t3\tsrc/main.rs\n\ndef5678\n10\t2\tsrc/main.rs\n";
        let result = parse_numstat(input);
        assert_eq!(result.len(), 1);
        let main = &result[0];
        assert_eq!(main.commits, 2);
        assert_eq!(main.lines_added, 15);
        assert_eq!(main.lines_deleted, 5);
    }

    #[test]
    fn test_parse_numstat_binary_files() {
        let input = "abc1234\n-\t-\timage.png\n5\t3\tsrc/main.rs\n";
        let result = parse_numstat(input);
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
    fn test_parse_numstat_rename_merges_history() {
        // newest-first: rename commit, then older history under the old name
        let input = "\
aaa111\n3\t1\tsrc/new.rs\n\n\
bbb222\n0\t0\tsrc/{old.rs => new.rs}\n\n\
ccc333\n10\t2\tsrc/old.rs\n\n\
ddd444\n5\t0\tsrc/old.rs\n";
        let result = parse_numstat(input);
        assert_eq!(result.len(), 1, "old and new names must merge: {result:?}");
        let f = &result[0];
        assert_eq!(f.path, PathBuf::from("src/new.rs"));
        assert_eq!(f.commits, 4);
        assert_eq!(f.lines_added, 18);
        assert_eq!(f.lines_deleted, 3);
    }

    #[test]
    fn test_parse_numstat_chained_rename() {
        // b => c (newer), then a => b (older): everything lands on c
        let input = "\
aaa\n0\t0\tb.rs => c.rs\n\n\
bbb\n0\t0\ta.rs => b.rs\n\n\
ccc\n7\t1\ta.rs\n";
        let result = parse_numstat(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("c.rs"));
        assert_eq!(result[0].commits, 3);
        assert_eq!(result[0].lines_added, 7);
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

    #[test]
    fn test_file_churn_empty_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let client = GitClient::detect(temp.path()).unwrap();
        let churns = client.file_churn("90 days ago").unwrap();
        assert!(churns.is_empty());
    }

    #[test]
    fn test_file_churn_with_commits() {
        let temp = tempfile::TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(temp.path())
            .output()
            .unwrap();

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
    }
}
