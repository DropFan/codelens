//! Trend tracking with snapshots.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::analyzer::stats::AnalysisResult;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::insight::DeltaValue;

/// Snapshots at or above this version store files relative to their snapshot
/// root when possible, so moving the project does not break file identity.
pub const PORTABLE_PATH_VERSION: u32 = 2;

/// Snapshots at or above this version can record the paths and
/// analysis-affecting options needed to decide whether two results are
/// comparable.
pub const ANALYSIS_SCOPE_VERSION: u32 = 3;

/// Inputs that can change which files or measurements an analysis contains.
/// Presentation-only settings such as output format, sorting, and thread count
/// are intentionally excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisScope {
    paths: Vec<PathBuf>,
    follow_symlinks: bool,
    use_gitignore: bool,
    max_depth: Option<usize>,
    custom_ignores: Vec<String>,
    excludes: Vec<String>,
    includes: Vec<String>,
    exclude_files: Vec<String>,
    include_files: Vec<String>,
    exclude_dirs: Vec<String>,
    languages: Vec<String>,
    min_lines: Option<usize>,
    max_lines: Option<usize>,
    smart_exclude: bool,
    include_all: bool,
    no_duplicates: bool,
    no_min_gen: bool,
    no_linguist: bool,
    count_as: Vec<(String, String)>,
    custom_languages_fingerprint: Option<String>,
    no_dup_scan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub timestamp: DateTime<Utc>,
    pub label: Option<String>,
    pub git_commit: Option<String>,
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_scope: Option<AnalysisScope>,
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
    save_snapshot_at(
        project_root,
        result,
        label,
        git_commit,
        git_branch,
        None,
        Utc::now(),
    )
}

/// Save a snapshot together with the analysis inputs required for safe future
/// comparisons. New command-line callers should prefer this over
/// [`save_snapshot`]; the older function remains available for API and legacy
/// snapshot compatibility.
pub fn save_snapshot_with_scope<P: AsRef<Path>>(
    project_root: &Path,
    result: AnalysisResult,
    paths: &[P],
    config: &Config,
    label: Option<String>,
    git_commit: Option<String>,
    git_branch: Option<String>,
) -> Result<PathBuf> {
    let scope = analysis_scope(project_root, paths, config)?;
    save_snapshot_at(
        project_root,
        result,
        label,
        git_commit,
        git_branch,
        Some(scope),
        Utc::now(),
    )
}

/// Capture the analysis-affecting subset of a configuration.
pub fn analysis_scope<P: AsRef<Path>>(
    project_root: &Path,
    paths: &[P],
    config: &Config,
) -> Result<AnalysisScope> {
    let root = fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    let mut seen_paths = std::collections::HashSet::new();
    let paths = paths
        .iter()
        .map(|path| scope_path(&root, path.as_ref()))
        .filter(|path| seen_paths.insert(path.clone()))
        .collect();
    let custom_languages_fingerprint = config
        .languages_file
        .as_deref()
        .map(|path| {
            fs::read(path)
                .map(|content| stable_fingerprint(&content))
                .map_err(|source| Error::FileRead {
                    path: path.to_path_buf(),
                    source,
                })
        })
        .transpose()?;

    Ok(AnalysisScope {
        paths,
        follow_symlinks: config.walker.follow_symlinks,
        use_gitignore: config.walker.use_gitignore,
        max_depth: config.walker.max_depth,
        custom_ignores: config.walker.custom_ignores.clone(),
        excludes: config.filter.excludes.clone(),
        includes: config.filter.includes.clone(),
        exclude_files: config.filter.exclude_files.clone(),
        include_files: config.filter.include_files.clone(),
        exclude_dirs: config.filter.exclude_dirs.clone(),
        languages: config.filter.languages.clone(),
        min_lines: config.filter.min_lines,
        max_lines: config.filter.max_lines,
        smart_exclude: config.filter.smart_exclude,
        include_all: config.filter.include_all,
        no_duplicates: config.filter.no_duplicates,
        no_min_gen: config.filter.no_min_gen,
        no_linguist: config.filter.no_linguist,
        count_as: config.count_as.clone(),
        custom_languages_fingerprint,
        no_dup_scan: config.no_dup_scan,
    })
}

fn scope_path(root: &Path, path: &Path) -> PathBuf {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    match absolute.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => PathBuf::from("."),
        Ok(relative) => relative.to_path_buf(),
        Err(_) => absolute,
    }
}

fn stable_fingerprint(content: &[u8]) -> String {
    // FNV-1a is deliberately implemented here instead of DefaultHasher: its
    // output is stable across Rust releases, which persisted snapshots need.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in content {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Reject a comparison only when both sides carry a scope contract and the
/// contracts differ. Scope-less historical snapshots remain readable.
pub fn ensure_compatible_analysis_scopes(
    from_scope: Option<&AnalysisScope>,
    to_scope: Option<&AnalysisScope>,
    from: impl Into<String>,
    to: impl Into<String>,
) -> Result<()> {
    if matches!((from_scope, to_scope), (Some(left), Some(right)) if left != right) {
        return Err(Error::SnapshotScopeMismatch {
            from: from.into(),
            to: to.into(),
        });
    }
    Ok(())
}

fn save_snapshot_at(
    project_root: &Path,
    mut result: AnalysisResult,
    label: Option<String>,
    git_commit: Option<String>,
    git_branch: Option<String>,
    analysis_scope: Option<AnalysisScope>,
    now: DateTime<Utc>,
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

    make_paths_relative(&mut result, project_root);
    let snapshot = Snapshot {
        version: if analysis_scope.is_some() {
            ANALYSIS_SCOPE_VERSION
        } else {
            PORTABLE_PATH_VERSION
        },
        timestamp: now,
        label,
        git_commit,
        git_branch,
        analysis_scope,
        result,
    };

    let json = serde_json::to_string_pretty(&snapshot)?;
    let stem = now.format("%Y-%m-%dT%H-%M-%S%.9fZ").to_string();
    for collision in 0usize.. {
        let filename = if collision == 0 {
            format!("{stem}.json")
        } else {
            format!("{stem}~{collision:020}.json")
        };
        let path = dir.join(filename);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(json.as_bytes())
                    .map_err(|source| Error::FileRead {
                        path: path.clone(),
                        source,
                    })?;
                return Ok(path);
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(Error::FileRead {
                    path: path.clone(),
                    source,
                });
            }
        }
    }
    unreachable!("the snapshot collision counter is unbounded")
}

fn make_paths_relative(result: &mut AnalysisResult, project_root: &Path) {
    let root = fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    let relative = |path: &Path| {
        fs::canonicalize(path)
            .ok()
            .and_then(|absolute| absolute.strip_prefix(&root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    };

    for file in &mut result.files {
        file.path = relative(&file.path);
    }
    let regions = std::mem::take(&mut result.summary.health_regions);
    result.summary.health_regions = regions
        .into_iter()
        .map(|(path, stats)| (relative(&path), stats))
        .collect();
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

    metas.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
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

    ensure_compatible_analysis_scopes(
        from_snap.analysis_scope.as_ref(),
        to_snap.analysis_scope.as_ref(),
        from_ref,
        to_ref,
    )?;

    let (delta, by_language) = compute_delta(&from_snap.result.summary, &to_snap.result.summary);

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

/// Metric and per-language deltas between two summaries.
/// Shared by trend (snapshot vs snapshot) and diff (git ref vs git ref).
pub fn compute_delta(
    from_summary: &crate::analyzer::stats::Summary,
    to_summary: &crate::analyzer::stats::Summary,
) -> (TrendDelta, Vec<LanguageTrend>) {
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

    (delta, by_language)
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
                duplicate_lines: 0,
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
    fn saved_snapshot_paths_are_relative_to_the_project_root() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn value() {}\n").unwrap();
        let result = crate::analyze(&[dir.path()], &crate::Config::default()).unwrap();

        let path = save_snapshot(dir.path(), result, None, None, None).unwrap();
        let snapshot = load_snapshot(&path).unwrap();

        assert_eq!(snapshot.version, PORTABLE_PATH_VERSION);
        assert_eq!(snapshot.result.files[0].path, Path::new("src/lib.rs"));
        assert!(snapshot
            .result
            .summary
            .health_regions
            .keys()
            .all(|path| !path.is_absolute()));
    }

    #[test]
    fn scoped_snapshot_records_analysis_affecting_options() {
        let dir = TempDir::new().unwrap();
        let languages_file = dir.path().join("languages.toml");
        std::fs::write(&languages_file, "[languages]\n").unwrap();
        let mut config = crate::Config::default();
        config.walker.follow_symlinks = true;
        config.walker.use_gitignore = false;
        config.walker.max_depth = Some(4);
        config.walker.custom_ignores = vec![".extraignore".into()];
        config.filter.excludes = vec!["vendor".into()];
        config.filter.includes = vec!["src/**".into()];
        config.filter.exclude_files = vec!["generated[.]rs$".into()];
        config.filter.include_files = vec!["[.]rs$".into()];
        config.filter.exclude_dirs = vec!["target$".into()];
        config.filter.languages = vec!["Rust".into()];
        config.filter.min_lines = Some(2);
        config.filter.max_lines = Some(200);
        config.filter.smart_exclude = true;
        config.filter.include_all = true;
        config.filter.no_duplicates = true;
        config.filter.no_min_gen = true;
        config.filter.no_linguist = true;
        config.count_as = vec![("inc".into(), "C".into())];
        config.languages_file = Some(languages_file.clone());
        config.no_dup_scan = true;

        let path = save_snapshot_with_scope(
            dir.path(),
            make_result(10, 1),
            &[dir.path()],
            &config,
            None,
            None,
            None,
        )
        .unwrap();
        let snapshot = load_snapshot(&path).unwrap();
        let scope = snapshot.analysis_scope.unwrap();

        assert_eq!(snapshot.version, ANALYSIS_SCOPE_VERSION);
        assert_eq!(scope.paths, vec![PathBuf::from(".")]);
        assert!(scope.follow_symlinks);
        assert!(!scope.use_gitignore);
        assert_eq!(scope.max_depth, Some(4));
        assert_eq!(scope.custom_ignores, vec![".extraignore"]);
        assert_eq!(scope.excludes, vec!["vendor"]);
        assert_eq!(scope.includes, vec!["src/**"]);
        assert_eq!(scope.exclude_files, vec!["generated[.]rs$"]);
        assert_eq!(scope.include_files, vec!["[.]rs$"]);
        assert_eq!(scope.exclude_dirs, vec!["target$"]);
        assert_eq!(scope.languages, vec!["Rust"]);
        assert_eq!(scope.min_lines, Some(2));
        assert_eq!(scope.max_lines, Some(200));
        assert!(scope.smart_exclude);
        assert!(scope.include_all);
        assert!(scope.no_duplicates);
        assert!(scope.no_min_gen);
        assert!(scope.no_linguist);
        assert_eq!(scope.count_as, vec![("inc".into(), "C".into())]);
        let expected_fingerprint = stable_fingerprint(b"[languages]\n");
        assert_eq!(
            scope.custom_languages_fingerprint.as_deref(),
            Some(expected_fingerprint.as_str())
        );
        assert!(scope.no_dup_scan);

        std::fs::write(languages_file, "[languages.Rust]\n").unwrap();
        assert_ne!(
            scope,
            analysis_scope(dir.path(), &[dir.path()], &config).unwrap()
        );
    }

    #[test]
    fn analysis_scope_is_stable_when_project_moves() {
        let parent = TempDir::new().unwrap();
        let original = parent.path().join("original");
        let moved = parent.path().join("moved");
        std::fs::create_dir_all(original.join("src")).unwrap();

        let before = analysis_scope(
            &original,
            &[original.join("src")],
            &crate::Config::default(),
        )
        .unwrap();
        let with_redundant_path = analysis_scope(
            &original,
            &[original.join("src"), original.join("src/.")],
            &crate::Config::default(),
        )
        .unwrap();
        assert_eq!(before, with_redundant_path);

        std::fs::rename(&original, &moved).unwrap();
        let after =
            analysis_scope(&moved, &[moved.join("src")], &crate::Config::default()).unwrap();

        assert_eq!(before, after);
    }

    #[test]
    fn snapshots_with_the_same_timestamp_never_overwrite() {
        let dir = TempDir::new().unwrap();
        let now = Utc::now();

        let first = save_snapshot_at(
            dir.path(),
            make_result(10, 1),
            Some("first".into()),
            None,
            None,
            None,
            now,
        )
        .unwrap();
        let second = save_snapshot_at(
            dir.path(),
            make_result(20, 2),
            Some("second".into()),
            None,
            None,
            None,
            now,
        )
        .unwrap();

        assert_ne!(first, second);
        assert_eq!(list_snapshots(dir.path()).unwrap().len(), 2);
        assert_eq!(
            load_snapshot(&first).unwrap().label.as_deref(),
            Some("first")
        );
        assert_eq!(
            load_snapshot(&second).unwrap().label.as_deref(),
            Some("second")
        );
        assert_eq!(resolve_snapshot(dir.path(), "latest").unwrap(), second);
        assert_eq!(resolve_snapshot(dir.path(), "latest~1").unwrap(), first);
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
    fn diff_rejects_snapshots_with_different_analysis_scopes() {
        let dir = TempDir::new().unwrap();
        let default = crate::Config::default();
        save_snapshot_with_scope(
            dir.path(),
            make_result(100, 5),
            &[dir.path()],
            &default,
            Some("before".into()),
            None,
            None,
        )
        .unwrap();
        let mut filtered = default.clone();
        filtered.filter.exclude_files = vec!["generated[.]rs$".into()];
        save_snapshot_with_scope(
            dir.path(),
            make_result(100, 5),
            &[dir.path()],
            &filtered,
            Some("after".into()),
            None,
            None,
        )
        .unwrap();

        let error = diff(dir.path(), "latest~1", "latest").unwrap_err();

        assert!(matches!(error, Error::SnapshotScopeMismatch { .. }));
        assert!(error.to_string().contains("analysis scope differs"));
    }

    #[test]
    fn diff_keeps_scope_less_snapshots_compatible() {
        let dir = TempDir::new().unwrap();
        save_snapshot(
            dir.path(),
            make_result(100, 5),
            Some("legacy".into()),
            None,
            None,
        )
        .unwrap();
        save_snapshot_with_scope(
            dir.path(),
            make_result(150, 7),
            &[dir.path()],
            &crate::Config::default(),
            Some("scoped".into()),
            None,
            None,
        )
        .unwrap();

        let report = diff(dir.path(), "latest~1", "latest").unwrap();

        assert_eq!(report.from.label.as_deref(), Some("legacy"));
        assert_eq!(report.to.label.as_deref(), Some("scoped"));
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
