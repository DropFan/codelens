//! Code health score analysis.
//!
//! Computes health scores at three levels: project, directory, and file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::analyzer::stats::{AnalysisResult, FileStats};
use crate::insight::scoring::{HealthDimension, RawMetrics, ScoringModel};
use crate::insight::Grade;

#[derive(Debug, Clone, Serialize)]
pub struct DimensionScore {
    pub dimension: HealthDimension,
    pub score: f64,
    pub grade: Grade,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileHealth {
    pub path: PathBuf,
    pub score: f64,
    pub grade: Grade,
    pub top_issue: HealthDimension,
    pub dimensions: Vec<DimensionScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryHealth {
    pub path: PathBuf,
    pub score: f64,
    pub grade: Grade,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub score: f64,
    pub grade: Grade,
    pub model: String,
    pub dimensions: Vec<DimensionScore>,
    pub by_directory: Vec<DirectoryHealth>,
    pub worst_files: Vec<FileHealth>,
    /// Delta against a baseline (--baseline); None for plain reports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regression: Option<RegressionReport>,
}

/// A file whose health grade dropped compared to the baseline.
#[derive(Debug, Clone, Serialize)]
pub struct FileRegression {
    pub path: PathBuf,
    pub from_score: f64,
    pub from_grade: Grade,
    pub to_score: f64,
    pub to_grade: Grade,
}

/// Health delta against a baseline tree (snapshot or git ref).
///
/// Follows the "clean as you code" gate philosophy: only letter-grade
/// drops fail, so legacy debt does not block CI — a file's grade can
/// only move when the file itself was touched.
#[derive(Debug, Clone, Serialize)]
pub struct RegressionReport {
    /// Human-readable baseline reference (snapshot id or git ref).
    pub baseline: String,
    pub baseline_score: f64,
    pub baseline_grade: Grade,
    /// Current score minus baseline score.
    pub score_delta: f64,
    /// Project letter grade dropped (e.g. B → C).
    pub project_regressed: bool,
    /// Files present in both trees whose letter grade dropped, worst first.
    pub regressed_files: Vec<FileRegression>,
    /// Files present in both trees whose letter grade improved.
    pub improved_files: usize,
    /// Gate verdict: project regressed or any file regressed.
    pub failed: bool,
}

/// Compare current analysis against a baseline tree with the same scoring
/// model. Only files present in both trees are compared (paths normalized
/// by stripping a leading "./"); added and removed files never regress.
pub fn compare_with_baseline(
    baseline: &AnalysisResult,
    current: &AnalysisResult,
    model: &dyn ScoringModel,
    baseline_label: &str,
) -> RegressionReport {
    let normalize = |p: &Path| -> PathBuf { p.strip_prefix("./").unwrap_or(p).to_path_buf() };

    let baseline_files: HashMap<PathBuf, (f64, Grade)> = baseline
        .files
        .iter()
        .map(|f| {
            let metrics = RawMetrics::from_file(f);
            let s = model.total_score(&metrics);
            (normalize(&f.path), (s, model.grade(s)))
        })
        .collect();

    let mut regressed_files = Vec::new();
    let mut improved_files = 0usize;
    for f in &current.files {
        let Some(&(from_score, from_grade)) = baseline_files.get(&normalize(&f.path)) else {
            continue;
        };
        let metrics = RawMetrics::from_file(f);
        let to_score = model.total_score(&metrics);
        let to_grade = model.grade(to_score);
        // Grade orders A < B < ... < F, so "greater" means worse.
        if to_grade > from_grade {
            regressed_files.push(FileRegression {
                path: f.path.clone(),
                from_score,
                from_grade,
                to_score,
                to_grade,
            });
        } else if to_grade < from_grade {
            improved_files += 1;
        }
    }
    regressed_files.sort_by(|a, b| {
        b.to_grade.cmp(&a.to_grade).then(
            a.to_score
                .partial_cmp(&b.to_score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let baseline_score = model.total_score(&RawMetrics::from_files(&baseline.files));
    let baseline_grade = model.grade(baseline_score);
    let current_score = model.total_score(&RawMetrics::from_files(&current.files));
    let current_grade = model.grade(current_score);
    let project_regressed = current_grade > baseline_grade;

    RegressionReport {
        baseline: baseline_label.to_string(),
        baseline_score,
        baseline_grade,
        score_delta: current_score - baseline_score,
        project_regressed,
        failed: project_regressed || !regressed_files.is_empty(),
        regressed_files,
        improved_files,
    }
}

/// Generate a health report from analysis results using the given scoring model.
pub fn score(result: &AnalysisResult, model: &dyn ScoringModel, top_n: usize) -> HealthReport {
    // File-level scoring
    let mut file_healths: Vec<FileHealth> =
        result.files.iter().map(|f| score_file(f, model)).collect();

    // Sort by score ascending (worst first)
    file_healths.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Directory-level scoring
    let mut dir_files: HashMap<PathBuf, Vec<&FileStats>> = HashMap::new();
    for file in &result.files {
        let dir = file.path.parent().unwrap_or(Path::new(".")).to_path_buf();
        dir_files.entry(dir).or_default().push(file);
    }

    let mut dir_healths: Vec<DirectoryHealth> = dir_files
        .iter()
        .map(|(dir, files)| {
            let metrics = RawMetrics::from_file_refs(files);
            let dir_score = model.total_score(&metrics);
            DirectoryHealth {
                path: dir.clone(),
                score: dir_score,
                grade: model.grade(dir_score),
                file_count: files.len(),
            }
        })
        .collect();

    dir_healths.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Project-level scoring
    let project_metrics = RawMetrics::from_files(&result.files);
    let project_score = model.total_score(&project_metrics);
    let project_dimensions = score_dimensions(&project_metrics, model);

    HealthReport {
        score: project_score,
        grade: model.grade(project_score),
        model: model.name().to_string(),
        dimensions: project_dimensions,
        by_directory: dir_healths.into_iter().take(top_n).collect(),
        worst_files: file_healths.into_iter().take(top_n).collect(),
        regression: None,
    }
}

fn score_file(file: &FileStats, model: &dyn ScoringModel) -> FileHealth {
    let metrics = RawMetrics::from_file(file);
    let dimensions = score_dimensions(&metrics, model);
    let total = model.total_score(&metrics);

    let top_issue = dimensions
        .iter()
        .min_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|d| d.dimension)
        .unwrap_or(HealthDimension::Complexity);

    FileHealth {
        path: file.path.clone(),
        score: total,
        grade: model.grade(total),
        top_issue,
        dimensions,
    }
}

fn score_dimensions(metrics: &RawMetrics, model: &dyn ScoringModel) -> Vec<DimensionScore> {
    model
        .dimensions()
        .iter()
        .map(|dw| {
            let s = model.score_dimension(dw.dimension, metrics);
            DimensionScore {
                dimension: dw.dimension,
                score: s,
                grade: model.grade(s),
                weight: dw.weight,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{Complexity, FileStats, LineStats, Summary};
    use crate::insight::scoring::default::DefaultModel;
    use std::time::Duration;

    fn make_test_result() -> AnalysisResult {
        let files = vec![
            FileStats {
                path: PathBuf::from("src/good.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 50,
                    code: 40,
                    comment: 5,
                    blank: 5,
                },
                size: 1000,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions: 3,
                    cyclomatic: 6,
                    cognitive: 0,
                    max_depth: 2,
                    avg_func_lines: 13.0,
                },
            },
            FileStats {
                path: PathBuf::from("src/bad.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 500,
                    code: 400,
                    comment: 10,
                    blank: 90,
                },
                size: 10000,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions: 2,
                    cyclomatic: 30,
                    cognitive: 0,
                    max_depth: 8,
                    avg_func_lines: 200.0,
                },
            },
            FileStats {
                path: PathBuf::from("lib/utils.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 80,
                    code: 60,
                    comment: 10,
                    blank: 10,
                },
                size: 1500,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions: 5,
                    cyclomatic: 10,
                    cognitive: 0,
                    max_depth: 3,
                    avg_func_lines: 12.0,
                },
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(50),
            scanned_files: 3,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn test_health_report_structure() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.model, "default");
        assert!(!report.dimensions.is_empty());
        assert!(!report.worst_files.is_empty());
        assert!(!report.by_directory.is_empty());
    }

    #[test]
    fn test_score_without_duplication_dimension() {
        // Unmeasured duplication: the dimension disappears from the
        // report entirely instead of showing a misleading perfect grade.
        let result = make_test_result();
        let model = DefaultModel::without_duplication();
        let report = score(&result, &model, 10);
        assert!(report
            .dimensions
            .iter()
            .all(|d| d.dimension != HealthDimension::Duplication));
        assert!(report.worst_files.iter().all(|f| {
            f.dimensions
                .iter()
                .all(|d| d.dimension != HealthDimension::Duplication)
        }));
    }

    #[test]
    fn test_worst_files_sorted_ascending() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        for window in report.worst_files.windows(2) {
            assert!(window[0].score <= window[1].score);
        }
    }

    #[test]
    fn test_bad_file_has_lower_score() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        let bad = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("bad.rs"))
            .unwrap();
        let good = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("good.rs"))
            .unwrap();
        assert!(bad.score < good.score);
    }

    #[test]
    fn test_directory_grouping() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.by_directory.len(), 2);
        let dir_paths: Vec<&Path> = report
            .by_directory
            .iter()
            .map(|d| d.path.as_path())
            .collect();
        assert!(dir_paths.contains(&Path::new("src")));
        assert!(dir_paths.contains(&Path::new("lib")));
    }

    #[test]
    fn test_top_n_limits() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 1);
        assert_eq!(report.worst_files.len(), 1);
        assert_eq!(report.by_directory.len(), 1);
    }

    #[test]
    fn test_empty_result() {
        let result = AnalysisResult {
            files: vec![],
            summary: Summary::default(),
            elapsed: Duration::from_millis(1),
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        };
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert!(report.worst_files.is_empty());
        assert!(report.by_directory.is_empty());
    }

    fn worsen(file: &mut FileStats) {
        file.lines.total = 900;
        file.lines.code = 850;
        file.complexity.cyclomatic = 120;
        file.complexity.functions = 2;
        file.complexity.max_depth = 9;
        file.complexity.avg_func_lines = 400.0;
    }

    #[test]
    fn test_compare_no_change_passes() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let reg = compare_with_baseline(&result, &result, &model, "latest");
        assert!(!reg.failed);
        assert!(!reg.project_regressed);
        assert!(reg.regressed_files.is_empty());
        assert!(reg.score_delta.abs() < 0.001);
    }

    #[test]
    fn test_compare_detects_file_regression() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        worsen(&mut current.files[0]); // good.rs degrades
        current.summary = Summary::from_file_stats(&current.files);

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(reg.failed);
        assert_eq!(reg.regressed_files.len(), 1);
        let fr = &reg.regressed_files[0];
        assert!(fr.path.ends_with("good.rs"));
        assert!(fr.to_grade > fr.from_grade);
    }

    #[test]
    fn test_compare_added_file_never_regresses() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        let mut extra = current.files[1].clone();
        extra.path = PathBuf::from("src/new_horror.rs");
        worsen(&mut extra);
        current.files.push(extra);
        current.summary = Summary::from_file_stats(&current.files);

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(
            reg.regressed_files.is_empty(),
            "files absent from the baseline must not appear as regressions"
        );
    }

    #[test]
    fn test_compare_counts_improvements() {
        let mut baseline = make_test_result();
        worsen(&mut baseline.files[0]);
        let current = make_test_result();

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(reg.improved_files >= 1);
        assert!(reg.regressed_files.is_empty());
    }

    #[test]
    fn test_compare_normalizes_dot_prefix() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        for f in &mut current.files {
            f.path = PathBuf::from("./").join(&f.path);
        }
        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(
            !reg.failed,
            "./-prefixed paths must match their baseline counterparts"
        );
    }

    #[test]
    fn test_file_top_issue() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        let bad = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("bad.rs"))
            .unwrap();
        assert!(bad.dimensions.iter().any(|d| d.dimension == bad.top_issue));
    }
}
