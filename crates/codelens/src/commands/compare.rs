//! Shared preparation for health comparisons.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::insight::scoring::{HealthModelVersion, ScoringModel};
use codelens_core::{analyze, AnalysisResult, Config, GitClient};

use super::{rewrite_paths_repo_relative, scoring_model_for};

pub(crate) struct PreparedComparison {
    pub(crate) baseline: AnalysisResult,
    pub(crate) current: AnalysisResult,
}

impl PreparedComparison {
    pub(crate) fn scoring_model(&self, version: HealthModelVersion) -> Box<dyn ScoringModel> {
        scoring_model_for(
            version,
            self.baseline.summary.dup_scanned && self.current.summary.dup_scanned,
        )
    }
}

/// Normalize and validate both sides before any score delta is calculated.
///
/// `repo_root` is optional because snapshot comparisons also work outside a
/// Git repository. Submodules are filtered symmetrically whenever a Git
/// context is available.
pub(crate) fn prepare_comparison(
    mut baseline: AnalysisResult,
    baseline_label: &str,
    mut current: AnalysisResult,
    current_label: &str,
    repo_root: Option<&Path>,
    submodules: &[PathBuf],
) -> Option<PreparedComparison> {
    if let Some(repo_root) = repo_root {
        rewrite_analysis_paths_repo_relative(&mut baseline, repo_root);
        rewrite_analysis_paths_repo_relative(&mut current, repo_root);
    }
    drop_submodule_files(&mut baseline, submodules);
    drop_submodule_files(&mut current, submodules);

    if !analysis_is_valid(baseline_label, &baseline) || !analysis_is_valid(current_label, &current)
    {
        return None;
    }

    Some(PreparedComparison { baseline, current })
}

/// A comparison is meaningful only when an input is complete and non-empty.
pub(crate) fn analysis_is_valid(label: &str, result: &AnalysisResult) -> bool {
    if result.error_files > 0 {
        eprintln!(
            "{}: analysis of '{label}' skipped {} file(s) because they could not be read or analyzed",
            "gate failed".red().bold(),
            result.error_files,
        );
        return false;
    }
    if result.summary.total_files == 0 {
        eprintln!(
            "{}: analysis of '{label}' found no files matching the requested filters",
            "gate failed".red().bold(),
        );
        return false;
    }
    true
}

/// Materialize a Git ref in a temporary worktree and analyze the same scope.
pub(crate) fn analyze_at_git_ref(
    git_client: &GitClient,
    reference: &str,
    paths: &[PathBuf],
    config: &Config,
) -> Result<AnalysisResult> {
    if !git_client.rev_exists(reference) {
        anyhow::bail!("'{reference}' is not a git ref in this repository");
    }

    let worktree = git_client.temp_worktree(reference)?;
    let repo_root = std::fs::canonicalize(git_client.repo_path())
        .unwrap_or_else(|_| git_client.repo_path().to_path_buf());
    let mut mapped = Vec::with_capacity(paths.len());
    for path in paths {
        let inside = std::fs::canonicalize(path).ok().and_then(|absolute| {
            absolute
                .strip_prefix(&repo_root)
                .ok()
                .map(|relative| worktree.path().join(relative))
        });
        match inside.filter(|mapped_path| mapped_path.exists()) {
            Some(mapped_path) => mapped.push(mapped_path),
            None => anyhow::bail!(
                "path '{}' does not exist in '{reference}'; compare a path that exists in both trees",
                path.display()
            ),
        }
    }

    let mut result = analyze(&mapped, config).context("Analysis of the git ref failed")?;
    rewrite_analysis_paths_repo_relative(&mut result, worktree.path());
    Ok(result)
}

/// Rewrite both physical paths and v2 health-region keys relative to a repo.
pub(crate) fn rewrite_analysis_paths_repo_relative(result: &mut AnalysisResult, repo_root: &Path) {
    rewrite_paths_repo_relative(&mut result.files, repo_root);

    let repo_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let regions = std::mem::take(&mut result.summary.health_regions);
    result.summary.health_regions = regions
        .into_iter()
        .map(|(path, stats)| {
            let path = std::fs::canonicalize(&path)
                .ok()
                .and_then(|absolute| {
                    absolute
                        .strip_prefix(&repo_root)
                        .ok()
                        .map(Path::to_path_buf)
                })
                .unwrap_or(path);
            (path, stats)
        })
        .collect();
}

/// Drop submodule files and rebuild derived summary fields on both sides.
pub(crate) fn drop_submodule_files(result: &mut AnalysisResult, submodules: &[PathBuf]) {
    if submodules.is_empty() {
        return;
    }
    result.files.retain(|file| {
        let path = file.path.strip_prefix("./").unwrap_or(&file.path);
        !submodules
            .iter()
            .any(|submodule| path.starts_with(submodule))
    });

    let dup_scanned = result.summary.dup_scanned;
    let uloc = result.summary.uloc;
    let inline_test_regions_scanned = result.summary.inline_test_regions_scanned;
    let mut health_regions = std::mem::take(&mut result.summary.health_regions);
    let retained_paths: HashSet<PathBuf> = result
        .files
        .iter()
        .map(|file| {
            file.path
                .strip_prefix("./")
                .unwrap_or(&file.path)
                .to_path_buf()
        })
        .collect();
    health_regions
        .retain(|path, _| retained_paths.contains(path.strip_prefix("./").unwrap_or(path)));
    result.summary = codelens_core::Summary::from_file_stats(&result.files);
    result.summary.dup_scanned = dup_scanned;
    result.summary.uloc = uloc;
    result.summary.inline_test_regions_scanned = inline_test_regions_scanned;
    result.summary.health_regions = health_regions;
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Duration;

    use codelens_core::insight::scoring::{HealthDimension, HealthModelVersion};
    use codelens_core::{AnalysisResult, FileStats, HealthRegions, Summary};

    use super::*;

    fn result(paths: &[&str], dup_scanned: bool, error_files: usize) -> AnalysisResult {
        let files = paths
            .iter()
            .map(|path| FileStats {
                path: PathBuf::from(path),
                language: "Rust".to_string(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut summary = Summary::from_file_stats(&files);
        summary.dup_scanned = dup_scanned;
        AnalysisResult {
            scanned_files: files.len(),
            files,
            summary,
            elapsed: Duration::ZERO,
            skipped_files: 0,
            error_files,
        }
    }

    #[test]
    fn preparation_rejects_empty_or_incomplete_either_side() {
        assert!(prepare_comparison(
            result(&[], true, 0),
            "baseline",
            result(&["src/current.rs"], true, 0),
            "current",
            None,
            &[],
        )
        .is_none());
        assert!(prepare_comparison(
            result(&["src/base.rs"], true, 0),
            "baseline",
            result(&["src/current.rs"], true, 1),
            "current",
            None,
            &[],
        )
        .is_none());
    }

    #[test]
    fn preparation_normalizes_paths_and_filters_submodules_symmetrically() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/lib.rs");
        let submodule = dir.path().join("vendor/lib/dep.rs");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(submodule.parent().unwrap()).unwrap();
        std::fs::write(&source, "fn source() {}\n").unwrap();
        std::fs::write(&submodule, "fn dependency() {}\n").unwrap();

        let mut baseline = result(&[], true, 0);
        baseline.files = vec![
            FileStats {
                path: source.clone(),
                language: "Rust".to_string(),
                ..Default::default()
            },
            FileStats {
                path: submodule.clone(),
                language: "Rust".to_string(),
                ..Default::default()
            },
        ];
        baseline.summary = Summary::from_file_stats(&baseline.files);
        baseline.summary.dup_scanned = true;
        baseline
            .summary
            .health_regions
            .insert(source.clone(), HealthRegions::default());
        baseline
            .summary
            .health_regions
            .insert(submodule.clone(), HealthRegions::default());
        let current = baseline.clone();

        let prepared = prepare_comparison(
            baseline,
            "baseline",
            current,
            "current",
            Some(dir.path()),
            &[PathBuf::from("vendor/lib")],
        )
        .unwrap();

        for analysis in [&prepared.baseline, &prepared.current] {
            assert_eq!(analysis.files.len(), 1);
            assert_eq!(analysis.files[0].path, Path::new("src/lib.rs"));
            assert!(analysis
                .summary
                .health_regions
                .contains_key(Path::new("src/lib.rs")));
        }
    }

    #[test]
    fn comparison_model_uses_only_dimensions_measured_on_both_sides() {
        let prepared = prepare_comparison(
            result(&["src/base.rs"], false, 0),
            "baseline",
            result(&["src/current.rs"], true, 0),
            "current",
            None,
            &[],
        )
        .unwrap();

        assert!(prepared
            .scoring_model(HealthModelVersion::V2)
            .dimensions()
            .iter()
            .all(|dimension| dimension.dimension != HealthDimension::Duplication));
    }

    #[test]
    fn git_ref_analysis_preserves_scope_and_rejects_missing_paths() {
        let repository = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(repository.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        let source_dir = repository.path().join("src");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::write(source_dir.join("lib.rs"), "pub fn committed() {}\n").unwrap();
        git(&["add", "src/lib.rs"]);
        git(&[
            "-c",
            "user.name=Codelens Test",
            "-c",
            "user.email=codelens@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "initial",
        ]);

        let client = GitClient::detect(repository.path()).unwrap();
        let analyzed = analyze_at_git_ref(
            &client,
            "HEAD",
            std::slice::from_ref(&source_dir),
            &Config::default(),
        )
        .unwrap();
        assert_eq!(analyzed.files.len(), 1);
        assert_eq!(analyzed.files[0].path, Path::new("src/lib.rs"));

        let uncommitted = source_dir.join("new.rs");
        std::fs::write(&uncommitted, "pub fn new_file() {}\n").unwrap();
        let error = analyze_at_git_ref(
            &client,
            "HEAD",
            std::slice::from_ref(&uncommitted),
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not exist in 'HEAD'"));
    }
}
