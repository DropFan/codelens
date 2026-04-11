//! Scoring model abstraction for code health analysis.

pub mod default;

use serde::Serialize;

use crate::analyzer::stats::FileStats;
use crate::insight::Grade;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum HealthDimension {
    Complexity,
    FuncSize,
    CommentRatio,
    FileSize,
    NestingDepth,
}

impl std::fmt::Display for HealthDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complexity => write!(f, "Complexity"),
            Self::FuncSize => write!(f, "Func Size"),
            Self::CommentRatio => write!(f, "Comment %"),
            Self::FileSize => write!(f, "File Size"),
            Self::NestingDepth => write!(f, "Nesting"),
        }
    }
}

pub struct DimensionWeight {
    pub dimension: HealthDimension,
    pub weight: f64,
}

#[derive(Debug, Clone, Default)]
pub struct RawMetrics {
    pub avg_cyclomatic: f64,
    pub avg_func_lines: f64,
    pub comment_ratio: f64,
    pub max_depth: usize,
    pub avg_file_lines: f64,
    pub total_files: usize,
}

impl RawMetrics {
    pub fn from_file(file: &FileStats) -> Self {
        let avg_cyclomatic = if file.complexity.functions > 0 {
            file.complexity.cyclomatic as f64 / file.complexity.functions as f64
        } else {
            0.0
        };
        let comment_ratio = if file.lines.code > 0 {
            file.lines.comment as f64 / file.lines.code as f64
        } else {
            0.0
        };
        Self {
            avg_cyclomatic,
            avg_func_lines: file.complexity.avg_func_lines,
            comment_ratio,
            max_depth: file.complexity.max_depth,
            avg_file_lines: file.lines.total as f64,
            total_files: 1,
        }
    }

    pub fn from_files(files: &[FileStats]) -> Self {
        if files.is_empty() {
            return Self::default();
        }
        let total_functions: usize = files.iter().map(|f| f.complexity.functions).sum();
        let total_cyclomatic: usize = files.iter().map(|f| f.complexity.cyclomatic).sum();
        let total_code: usize = files.iter().map(|f| f.lines.code).sum();
        let total_comment: usize = files.iter().map(|f| f.lines.comment).sum();
        let total_lines: usize = files.iter().map(|f| f.lines.total).sum();
        let max_depth = files.iter().map(|f| f.complexity.max_depth).max().unwrap_or(0);

        let avg_cyclomatic = if total_functions > 0 { total_cyclomatic as f64 / total_functions as f64 } else { 0.0 };
        let avg_func_lines = if total_functions > 0 { total_code as f64 / total_functions as f64 } else { 0.0 };
        let comment_ratio = if total_code > 0 { total_comment as f64 / total_code as f64 } else { 0.0 };
        let avg_file_lines = total_lines as f64 / files.len() as f64;

        Self { avg_cyclomatic, avg_func_lines, comment_ratio, max_depth, avg_file_lines, total_files: files.len() }
    }
}

pub trait ScoringModel: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> &[DimensionWeight];
    fn score_dimension(&self, dimension: HealthDimension, metrics: &RawMetrics) -> f64;

    fn grade(&self, score: f64) -> Grade {
        match score as u32 {
            90..=100 => Grade::A,
            80..=89 => Grade::B,
            70..=79 => Grade::C,
            60..=69 => Grade::D,
            _ => Grade::F,
        }
    }

    fn total_score(&self, metrics: &RawMetrics) -> f64 {
        let dims = self.dimensions();
        let total_weight: f64 = dims.iter().map(|d| d.weight).sum();
        if total_weight == 0.0 { return 0.0; }
        let weighted_sum: f64 = dims.iter().map(|d| self.score_dimension(d.dimension, metrics) * d.weight).sum();
        (weighted_sum / total_weight).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{Complexity, LineStats};
    use std::path::PathBuf;

    #[test]
    fn test_raw_metrics_from_file() {
        let file = FileStats {
            path: PathBuf::from("test.rs"),
            language: "Rust".to_string(),
            lines: LineStats { total: 100, code: 80, comment: 10, blank: 10 },
            size: 2000,
            complexity: Complexity { functions: 4, cyclomatic: 12, max_depth: 3, avg_func_lines: 20.0 },
        };
        let metrics = RawMetrics::from_file(&file);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        assert!((metrics.comment_ratio - 0.125).abs() < 0.01);
        assert_eq!(metrics.max_depth, 3);
    }

    #[test]
    fn test_raw_metrics_from_empty_files() {
        let metrics = RawMetrics::from_files(&[]);
        assert_eq!(metrics.total_files, 0);
    }

    #[test]
    fn test_raw_metrics_from_multiple_files() {
        let files = vec![
            FileStats {
                path: PathBuf::from("a.rs"), language: "Rust".to_string(),
                lines: LineStats { total: 100, code: 80, comment: 10, blank: 10 }, size: 2000,
                complexity: Complexity { functions: 4, cyclomatic: 12, max_depth: 3, avg_func_lines: 20.0 },
            },
            FileStats {
                path: PathBuf::from("b.rs"), language: "Rust".to_string(),
                lines: LineStats { total: 50, code: 40, comment: 5, blank: 5 }, size: 1000,
                complexity: Complexity { functions: 2, cyclomatic: 6, max_depth: 5, avg_func_lines: 20.0 },
            },
        ];
        let metrics = RawMetrics::from_files(&files);
        assert_eq!(metrics.total_files, 2);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        assert_eq!(metrics.max_depth, 5);
        assert!((metrics.avg_file_lines - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_health_dimension_display() {
        assert_eq!(HealthDimension::Complexity.to_string(), "Complexity");
        assert_eq!(HealthDimension::CommentRatio.to_string(), "Comment %");
    }
}
