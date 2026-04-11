//! Code health score analysis.
//!
//! Computes health scores at three levels: project, directory, and file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::analyzer::stats::{AnalysisResult, FileStats};
use crate::insight::Grade;
use crate::insight::scoring::{HealthDimension, RawMetrics, ScoringModel};

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
}

/// Generate a health report from analysis results using the given scoring model.
pub fn score(result: &AnalysisResult, model: &dyn ScoringModel, top_n: usize) -> HealthReport {
    // File-level scoring
    let mut file_healths: Vec<FileHealth> = result
        .files
        .iter()
        .map(|f| score_file(f, model))
        .collect();

    // Sort by score ascending (worst first)
    file_healths.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

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

    dir_healths.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

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
    }
}

fn score_file(file: &FileStats, model: &dyn ScoringModel) -> FileHealth {
    let metrics = RawMetrics::from_file(file);
    let dimensions = score_dimensions(&metrics, model);
    let total = model.total_score(&metrics);

    let top_issue = dimensions
        .iter()
        .min_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
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
                lines: LineStats { total: 50, code: 40, comment: 5, blank: 5 },
                size: 1000,
                complexity: Complexity { functions: 3, cyclomatic: 6, max_depth: 2, avg_func_lines: 13.0 },
            },
            FileStats {
                path: PathBuf::from("src/bad.rs"),
                language: "Rust".to_string(),
                lines: LineStats { total: 500, code: 400, comment: 10, blank: 90 },
                size: 10000,
                complexity: Complexity { functions: 2, cyclomatic: 30, max_depth: 8, avg_func_lines: 200.0 },
            },
            FileStats {
                path: PathBuf::from("lib/utils.rs"),
                language: "Rust".to_string(),
                lines: LineStats { total: 80, code: 60, comment: 10, blank: 10 },
                size: 1500,
                complexity: Complexity { functions: 5, cyclomatic: 10, max_depth: 3, avg_func_lines: 12.0 },
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(50),
            scanned_files: 3,
            skipped_files: 0,
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
        let bad = report.worst_files.iter().find(|f| f.path.ends_with("bad.rs")).unwrap();
        let good = report.worst_files.iter().find(|f| f.path.ends_with("good.rs")).unwrap();
        assert!(bad.score < good.score);
    }

    #[test]
    fn test_directory_grouping() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.by_directory.len(), 2);
        let dir_paths: Vec<&Path> = report.by_directory.iter().map(|d| d.path.as_path()).collect();
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
            files: vec![], summary: Summary::default(),
            elapsed: Duration::from_millis(1), scanned_files: 0, skipped_files: 0,
        };
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert!(report.worst_files.is_empty());
        assert!(report.by_directory.is_empty());
    }

    #[test]
    fn test_file_top_issue() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        let bad = report.worst_files.iter().find(|f| f.path.ends_with("bad.rs")).unwrap();
        assert!(bad.dimensions.iter().any(|d| d.dimension == bad.top_issue));
    }
}
