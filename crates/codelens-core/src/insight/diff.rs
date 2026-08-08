//! Diff two analyzed trees (git refs or working tree).
//!
//! Line counts between two commits are already covered by `git diff`
//! and the GitHub UI — the value here is what git cannot show: health
//! score movement, per-file grade regressions, and complexity deltas.

use serde::Serialize;

use crate::analyzer::stats::AnalysisResult;
use crate::insight::health::{self, RegressionReport};
use crate::insight::scoring::ScoringModel;
use crate::insight::trend::{compute_delta, LanguageTrend, TrendDelta};
use crate::insight::Grade;

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    /// Human-readable ref labels (e.g. "main", "HEAD", "worktree").
    pub from: String,
    pub to: String,
    /// Health of the `to` side.
    pub to_score: f64,
    pub to_grade: Grade,
    /// Health movement, regressed/improved files (baseline = `from`).
    pub health: RegressionReport,
    /// Metric deltas (lines, complexity, functions).
    pub delta: TrendDelta,
    /// Per-language code movement, largest first.
    pub by_language: Vec<LanguageTrend>,
}

/// Assemble a diff report from two analyzed trees.
pub fn build(
    from_label: &str,
    to_label: &str,
    from_result: &AnalysisResult,
    to_result: &AnalysisResult,
    model: &dyn ScoringModel,
) -> DiffReport {
    let health = health::compare_with_baseline(from_result, to_result, model, from_label);
    let (delta, by_language) = compute_delta(&from_result.summary, &to_result.summary);
    let to_score = health.baseline_score + health.score_delta;

    DiffReport {
        from: from_label.to_string(),
        to: to_label.to_string(),
        to_score,
        to_grade: model.grade(to_score),
        health,
        delta,
        by_language,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{Complexity, FileStats, LineStats, Summary};
    use crate::insight::scoring::default::DefaultModel;
    use std::path::PathBuf;
    use std::time::Duration;

    fn result_with(files: Vec<FileStats>) -> AnalysisResult {
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            scanned_files: files.len(),
            files,
            elapsed: Duration::from_millis(1),
            skipped_files: 0,
            error_files: 0,
        }
    }

    fn file(path: &str, code: usize, cyclomatic: usize) -> FileStats {
        FileStats {
            path: PathBuf::from(path),
            language: "Rust".to_string(),
            lines: LineStats {
                total: code + 10,
                code,
                comment: 5,
                blank: 5,
            },
            size: 100,
            duplicate_lines: 0,
            complexity: Complexity {
                functions: 3,
                cyclomatic,
                cognitive: cyclomatic,
                max_depth: 2,
                avg_func_lines: 15.0,
            },
        }
    }

    #[test]
    fn test_diff_report_deltas_and_health() {
        let from = result_with(vec![file("src/a.rs", 100, 6)]);
        let to = result_with(vec![file("src/a.rs", 150, 9), file("src/b.rs", 50, 3)]);

        let report = build("main", "HEAD", &from, &to, &DefaultModel::new());
        assert_eq!(report.from, "main");
        assert_eq!(report.delta.code.signed_delta(), 100);
        assert_eq!(report.delta.files.signed_delta(), 1);
        assert_eq!(report.health.baseline, "main");
        assert!(
            !report.health.failed,
            "modest growth must not read as regression"
        );
        assert!(
            (report.to_score - report.health.baseline_score - report.health.score_delta).abs()
                < 0.001
        );
    }
}
