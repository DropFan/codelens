//! Trend tracking with snapshots.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::analyzer::stats::AnalysisResult;
use crate::error::{Error, Result};
use crate::insight::DeltaValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub timestamp: DateTime<Utc>,
    pub label: Option<String>,
    pub git_commit: Option<String>,
    pub git_branch: Option<String>,
    pub result: AnalysisResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotMeta {
    pub timestamp: DateTime<Utc>,
    pub label: Option<String>,
    pub git_commit: Option<String>,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LangStatus {
    Added,
    Removed,
    Changed,
}

impl std::fmt::Display for LangStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LangStatus::Added => write!(f, "+"),
            LangStatus::Removed => write!(f, "-"),
            LangStatus::Changed => write!(f, "~"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageTrend {
    pub language: String,
    pub status: LangStatus,
    pub files: DeltaValue<usize>,
    pub code: DeltaValue<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendDelta {
    pub files: DeltaValue<usize>,
    pub lines: DeltaValue<usize>,
    pub code: DeltaValue<usize>,
    pub comment: DeltaValue<usize>,
    pub blank: DeltaValue<usize>,
    pub complexity: DeltaValue<usize>,
    pub functions: DeltaValue<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendReport {
    pub from: SnapshotMeta,
    pub to: SnapshotMeta,
    pub delta: TrendDelta,
    pub by_language: Vec<LanguageTrend>,
    /// Full snapshot history (oldest first) for charting; empty when not
    /// loaded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryPoint>,
}

/// One snapshot's headline metrics, for history charts.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryPoint {
    pub timestamp: DateTime<Utc>,
    pub label: Option<String>,
    pub files: usize,
    pub code: usize,
    pub cyclomatic: usize,
}

/// Load every snapshot's headline metrics, oldest first.
pub fn history(project_root: &Path) -> Result<Vec<HistoryPoint>> {
    let metas = list_snapshots(project_root)?;
    let mut points = Vec::with_capacity(metas.len());
    for meta in metas {
        let Ok(snapshot) = load_snapshot(&meta.file_path) else {
            continue;
        };
        points.push(HistoryPoint {
            timestamp: snapshot.timestamp,
            label: snapshot.label,
            files: snapshot.result.summary.total_files,
            code: snapshot.result.summary.lines.code,
            cyclomatic: snapshot.result.summary.complexity.cyclomatic,
        });
    }
    Ok(points)
}

fn snapshots_dir(project_root: &Path) -> PathBuf {
    project_root.join(".codelens").join("snapshots")
}

pub fn save_snapshot(
    project_root: &Path,
    result: AnalysisResult,
    label: Option<String>,
    git_commit: Option<String>,
    git_branch: Option<String>,
) -> Result<PathBuf> {
    let dir = snapshots_dir(project_root);
    fs::create_dir_all(&dir).map_err(|e| Error::FileRead {
        path: dir.clone(),
        source: e,
    })?;

    let gitignore = project_root.join(".codelens").join(".gitignore");
    if !gitignore.exists() {
        let _ = fs::write(
            &gitignore,
            "# codelens snapshots - uncomment the next line to stop tracking\n# *\n",
        );
    }

    let now = Utc::now();
    let snapshot = Snapshot {
        version: 1,
        timestamp: now,
        label,
        git_commit,
        git_branch,
        result,
    };

    let filename = now.format("%Y-%m-%dT%H-%M-%SZ").to_string() + ".json";
    let path = dir.join(&filename);
    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(&path, json).map_err(|e| Error::FileRead {
        path: path.clone(),
        source: e,
    })?;

    Ok(path)
}

pub fn list_snapshots(project_root: &Path) -> Result<Vec<SnapshotMeta>> {
    let dir = snapshots_dir(project_root);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut metas = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| Error::FileRead {
        path: dir.clone(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::FileRead {
            path: dir.clone(),
            source: e,
        })?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(meta) = read_snapshot_meta(&path) {
                metas.push(meta);
            }
        }
    }

    metas.sort_by_key(|m| m.timestamp);
    Ok(metas)
}

fn read_snapshot_meta(path: &Path) -> Result<SnapshotMeta> {
    let content = fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let snapshot: Snapshot = serde_json::from_str(&content)?;
    Ok(SnapshotMeta {
        timestamp: snapshot.timestamp,
        label: snapshot.label,
        git_commit: snapshot.git_commit,
        file_path: path.to_path_buf(),
    })
}

pub fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let content = fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let snapshot: Snapshot = serde_json::from_str(&content)?;
    Ok(snapshot)
}

pub fn resolve_snapshot(project_root: &Path, reference: &str) -> Result<PathBuf> {
    let metas = list_snapshots(project_root)?;
    if metas.is_empty() {
        return Err(Error::NoSnapshots {
            path: snapshots_dir(project_root),
        });
    }

    if reference == "latest" {
        return Ok(metas.last().unwrap().file_path.clone());
    }

    if let Some(offset_str) = reference.strip_prefix("latest~") {
        let offset: usize = offset_str.parse().map_err(|_| Error::SnapshotNotFound {
            id: reference.to_string(),
        })?;
        let idx = metas
            .len()
            .checked_sub(1 + offset)
            .ok_or(Error::SnapshotNotFound {
                id: reference.to_string(),
            })?;
        return Ok(metas[idx].file_path.clone());
    }

    // Date prefix match
    for meta in metas.iter().rev() {
        let ts = meta.timestamp.format("%Y-%m-%d").to_string();
        if ts.starts_with(reference) {
            return Ok(meta.file_path.clone());
        }
    }

    Err(Error::SnapshotNotFound {
        id: reference.to_string(),
    })
}

pub fn diff(project_root: &Path, from_ref: &str, to_ref: &str) -> Result<TrendReport> {
    let from_path = resolve_snapshot(project_root, from_ref)?;
    let to_path = resolve_snapshot(project_root, to_ref)?;

    let from_snap = load_snapshot(&from_path)?;
    let to_snap = load_snapshot(&to_path)?;

    let from_summary = &from_snap.result.summary;
    let to_summary = &to_snap.result.summary;

    let delta = TrendDelta {
        files: DeltaValue::new(from_summary.total_files, to_summary.total_files),
        lines: DeltaValue::new(from_summary.lines.total, to_summary.lines.total),
        code: DeltaValue::new(from_summary.lines.code, to_summary.lines.code),
        comment: DeltaValue::new(from_summary.lines.comment, to_summary.lines.comment),
        blank: DeltaValue::new(from_summary.lines.blank, to_summary.lines.blank),
        complexity: DeltaValue::new(
            from_summary.complexity.cyclomatic,
            to_summary.complexity.cyclomatic,
        ),
        functions: DeltaValue::new(
            from_summary.complexity.functions,
            to_summary.complexity.functions,
        ),
    };

    let mut by_language = Vec::new();
    let mut seen_langs = std::collections::HashSet::new();

    for (lang, to_stats) in &to_summary.by_language {
        seen_langs.insert(lang.clone());
        if let Some(from_stats) = from_summary.by_language.get(lang) {
            by_language.push(LanguageTrend {
                language: lang.clone(),
                status: LangStatus::Changed,
                files: DeltaValue::new(from_stats.files, to_stats.files),
                code: DeltaValue::new(from_stats.lines.code, to_stats.lines.code),
            });
        } else {
            by_language.push(LanguageTrend {
                language: lang.clone(),
                status: LangStatus::Added,
                files: DeltaValue::new(0, to_stats.files),
                code: DeltaValue::new(0, to_stats.lines.code),
            });
        }
    }

    for (lang, from_stats) in &from_summary.by_language {
        if !seen_langs.contains(lang) {
            by_language.push(LanguageTrend {
                language: lang.clone(),
                status: LangStatus::Removed,
                files: DeltaValue::new(from_stats.files, 0),
                code: DeltaValue::new(from_stats.lines.code, 0),
            });
        }
    }

    by_language.sort_by(|a, b| {
        b.code
            .signed_delta()
            .unsigned_abs()
            .cmp(&a.code.signed_delta().unsigned_abs())
    });

    Ok(TrendReport {
        from: SnapshotMeta {
            timestamp: from_snap.timestamp,
            label: from_snap.label,
            git_commit: from_snap.git_commit,
            file_path: from_path,
        },
        to: SnapshotMeta {
            timestamp: to_snap.timestamp,
            label: to_snap.label,
            git_commit: to_snap.git_commit,
            file_path: to_path,
        },
        delta,
        by_language,
        history: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{FileStats, LineStats, Summary};
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_result(code: usize, files: usize) -> AnalysisResult {
        let file_stats: Vec<FileStats> = (0..files)
            .map(|i| FileStats {
                path: PathBuf::from(format!("file_{i}.rs")),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: code / files.max(1),
                    code: code / files.max(1),
                    comment: 0,
                    blank: 0,
                },
                size: 1000,
                complexity: Default::default(),
            })
            .collect();
        AnalysisResult {
            summary: Summary::from_file_stats(&file_stats),
            files: file_stats,
            elapsed: Duration::from_millis(50),
            scanned_files: files,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn test_save_and_list() {
        let dir = TempDir::new().unwrap();
        let result = make_result(100, 2);
        let path = save_snapshot(dir.path(), result, Some("v1.0".into()), None, None).unwrap();
        assert!(path.exists());
        let metas = list_snapshots(dir.path()).unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].label.as_deref(), Some("v1.0"));
    }

    #[test]
    fn test_gitignore_created() {
        let dir = TempDir::new().unwrap();
        save_snapshot(dir.path(), make_result(10, 1), None, None, None).unwrap();
        assert!(dir.path().join(".codelens/.gitignore").exists());
    }

    #[test]
    fn test_resolve_latest() {
        let dir = TempDir::new().unwrap();
        save_snapshot(dir.path(), make_result(10, 1), None, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100)); // ensure different second in timestamp
        save_snapshot(
            dir.path(),
            make_result(20, 2),
            Some("second".into()),
            None,
            None,
        )
        .unwrap();
        let path = resolve_snapshot(dir.path(), "latest").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"scanned_files\": 2"));
    }

    #[test]
    fn test_resolve_latest_offset() {
        let dir = TempDir::new().unwrap();
        save_snapshot(
            dir.path(),
            make_result(10, 1),
            Some("first".into()),
            None,
            None,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        save_snapshot(
            dir.path(),
            make_result(20, 2),
            Some("second".into()),
            None,
            None,
        )
        .unwrap();
        let path = resolve_snapshot(dir.path(), "latest~1").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"scanned_files\": 1"));
    }

    #[test]
    fn test_resolve_not_found() {
        let dir = TempDir::new().unwrap();
        let result = resolve_snapshot(dir.path(), "latest");
        assert!(result.is_err());
    }

    #[test]
    fn test_diff() {
        let dir = TempDir::new().unwrap();
        save_snapshot(
            dir.path(),
            make_result(100, 5),
            Some("v1".into()),
            None,
            None,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        save_snapshot(
            dir.path(),
            make_result(150, 7),
            Some("v2".into()),
            None,
            None,
        )
        .unwrap();
        let report = diff(dir.path(), "latest~1", "latest").unwrap();
        assert_eq!(report.from.label.as_deref(), Some("v1"));
        assert_eq!(report.to.label.as_deref(), Some("v2"));
        assert_eq!(report.delta.files.from, 5);
        assert_eq!(report.delta.files.to, 7);
        // 100/5=20 per file * 5 = 100 total; 150/7=21 per file * 7 = 147 total; delta = 47
        assert_eq!(report.delta.code.signed_delta(), 47);
    }

    #[test]
    fn test_list_empty() {
        let dir = TempDir::new().unwrap();
        let metas = list_snapshots(dir.path()).unwrap();
        assert!(metas.is_empty());
    }

    #[test]
    fn test_lang_status_display() {
        assert_eq!(LangStatus::Added.to_string(), "+");
        assert_eq!(LangStatus::Removed.to_string(), "-");
        assert_eq!(LangStatus::Changed.to_string(), "~");
    }
}
