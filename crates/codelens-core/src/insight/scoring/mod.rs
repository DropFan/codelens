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
    Duplication,
}

impl std::fmt::Display for HealthDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complexity => write!(f, "Complexity"),
            Self::FuncSize => write!(f, "Func Size"),
            Self::CommentRatio => write!(f, "Comment %"),
            Self::FileSize => write!(f, "File Size"),
            Self::NestingDepth => write!(f, "Nesting"),
            Self::Duplication => write!(f, "Duplication"),
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
    /// For single file: the file's max nesting depth.
    /// For multiple files: P90 percentile of all files' max depths (robust against outliers).
    pub depth: usize,
    pub avg_file_lines: f64,
    pub total_files: usize,
    /// Duplicated line instances / non-blank lines (0.0 when duplication
    /// data is absent, e.g. snapshots from older versions). Analyses that
    /// explicitly skipped collection (--no-dup-scan, summary.dup_scanned
    /// is false) should instead be scored with a model that excludes the
    /// Duplication dimension (`DefaultModel::without_duplication`), so
    /// "not measured" never masquerades as a perfect score.
    pub duplication_ratio: f64,
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
        let non_blank = file.lines.total.saturating_sub(file.lines.blank);
        let duplication_ratio = if non_blank > 0 {
            file.duplicate_lines as f64 / non_blank as f64
        } else {
            0.0
        };
        Self {
            avg_cyclomatic,
            avg_func_lines: file.complexity.avg_func_lines,
            comment_ratio,
            depth: file.complexity.max_depth,
            avg_file_lines: file.lines.total as f64,
            total_files: 1,
            duplication_ratio,
        }
    }

    pub fn from_file_refs(files: &[&FileStats]) -> Self {
        if files.is_empty() {
            return Self::default();
        }
        let total_functions: usize = files.iter().map(|f| f.complexity.functions).sum();
        let total_cyclomatic: usize = files.iter().map(|f| f.complexity.cyclomatic).sum();
        let total_code: usize = files.iter().map(|f| f.lines.code).sum();
        let total_comment: usize = files.iter().map(|f| f.lines.comment).sum();
        let total_lines: usize = files.iter().map(|f| f.lines.total).sum();

        let mut depths: Vec<usize> = files.iter().map(|f| f.complexity.max_depth).collect();
        let depth = percentile_90(&mut depths);

        let avg_cyclomatic = if total_functions > 0 {
            total_cyclomatic as f64 / total_functions as f64
        } else {
            0.0
        };

        // Average function length considers only files that HAVE functions:
        // documents (Markdown/HTML/TOML) contribute code lines but never
        // functions, and would otherwise inflate the average.
        let func_file_code: usize = files
            .iter()
            .filter(|f| f.complexity.functions > 0)
            .map(|f| f.lines.code)
            .sum();
        let avg_func_lines = if total_functions > 0 {
            func_file_code as f64 / total_functions as f64
        } else {
            0.0
        };

        let comment_ratio = if total_code > 0 {
            total_comment as f64 / total_code as f64
        } else {
            0.0
        };
        let avg_file_lines = total_lines as f64 / files.len() as f64;

        let total_blank: usize = files.iter().map(|f| f.lines.blank).sum();
        let total_dup: usize = files.iter().map(|f| f.duplicate_lines).sum();
        let non_blank = total_lines.saturating_sub(total_blank);
        let duplication_ratio = if non_blank > 0 {
            total_dup as f64 / non_blank as f64
        } else {
            0.0
        };

        Self {
            avg_cyclomatic,
            avg_func_lines,
            comment_ratio,
            depth,
            avg_file_lines,
            total_files: files.len(),
            duplication_ratio,
        }
    }

    pub fn from_files(files: &[FileStats]) -> Self {
        let refs: Vec<&FileStats> = files.iter().collect();
        Self::from_file_refs(&refs)
    }
}

/// Compute the P90 percentile of a mutable slice (sorts in place).
/// For a single element, returns that element. For empty, returns 0.
fn percentile_90(values: &mut [usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let idx = ((values.len() as f64 - 1.0) * 0.9).ceil() as usize;
    values[idx.min(values.len() - 1)]
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
        if total_weight == 0.0 {
            return 0.0;
        }
        let weighted_sum: f64 = dims
            .iter()
            .map(|d| self.score_dimension(d.dimension, metrics) * d.weight)
            .sum();
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
            lines: LineStats {
                total: 100,
                code: 80,
                comment: 10,
                blank: 10,
            },
            size: 2000,
            duplicate_lines: 0,
            complexity: Complexity {
                functions: 4,
                cyclomatic: 12,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 20.0,
            },
        };
        let metrics = RawMetrics::from_file(&file);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        assert!((metrics.comment_ratio - 0.125).abs() < 0.01);
        assert_eq!(metrics.depth, 3);
    }

    #[test]
    fn test_avg_func_lines_ignores_files_without_functions() {
        // A Rust file: 200 code lines, 10 functions → avg 20
        let code = FileStats {
            path: PathBuf::from("lib.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 220,
                code: 200,
                comment: 10,
                blank: 10,
            },
            size: 5000,
            duplicate_lines: 0,
            complexity: Complexity {
                functions: 10,
                cyclomatic: 20,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 20.0,
            },
        };
        // A big Markdown doc: lots of "code" lines, zero functions
        let doc = FileStats {
            path: PathBuf::from("README.md"),
            language: "Markdown".to_string(),
            lines: LineStats {
                total: 3000,
                code: 2800,
                comment: 0,
                blank: 200,
            },
            size: 90_000,
            duplicate_lines: 0,
            complexity: Complexity::default(),
        };

        let metrics = RawMetrics::from_files(&[code, doc]);
        assert!(
            (metrics.avg_func_lines - 20.0).abs() < 0.01,
            "document lines must not inflate avg function length, got {}",
            metrics.avg_func_lines
        );
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
                path: PathBuf::from("a.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions: 4,
                    cyclomatic: 12,
                    cognitive: 0,
                    max_depth: 3,
                    avg_func_lines: 20.0,
                },
            },
            FileStats {
                path: PathBuf::from("b.rs"),
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
                    functions: 2,
                    cyclomatic: 6,
                    cognitive: 0,
                    max_depth: 5,
                    avg_func_lines: 20.0,
                },
            },
        ];
        let metrics = RawMetrics::from_files(&files);
        assert_eq!(metrics.total_files, 2);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        // P90 of [3, 5] = 5 (only 2 elements, P90 picks the higher)
        assert_eq!(metrics.depth, 5);
        assert!((metrics.avg_file_lines - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_health_dimension_display() {
        assert_eq!(HealthDimension::Complexity.to_string(), "Complexity");
        assert_eq!(HealthDimension::CommentRatio.to_string(), "Comment %");
    }

    #[test]
    fn test_percentile_90_empty() {
        assert_eq!(super::percentile_90(&mut []), 0);
    }

    #[test]
    fn test_percentile_90_single() {
        assert_eq!(super::percentile_90(&mut [7]), 7);
    }

    #[test]
    fn test_percentile_90_filters_outlier() {
        // 10 files: 9 with depth 3, 1 outlier with depth 15
        // P90 index = ceil(9 * 0.9) = 9 → sorted[9] = 15
        // But with 10 elements: ceil((10-1)*0.9) = ceil(8.1) = 9 → sorted[9] = 15
        // To actually filter: need more normal values.
        // 20 files: 18 with depth 3, 2 outliers with depth 15
        let mut depths = vec![3; 18];
        depths.extend_from_slice(&[15, 15]);
        // P90 index = ceil(19 * 0.9) = ceil(17.1) = 18 → sorted[18] = 15
        // Still picks outlier. Need 90%+ to be normal.
        // 20 files: 19 with depth 3, 1 outlier with depth 15
        let mut depths = vec![3; 19];
        depths.push(15);
        // P90 index = ceil(19 * 0.9) = ceil(17.1) = 18 → sorted[18] = 3
        assert_eq!(super::percentile_90(&mut depths), 3);
    }

    #[test]
    fn test_percentile_90_gradual() {
        // depths: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        // P90 index = ceil(9 * 0.9) = ceil(8.1) = 9 → sorted[9] = 10
        let mut depths: Vec<usize> = (1..=10).collect();
        assert_eq!(super::percentile_90(&mut depths), 10);

        // depths: [1, 2, 3, ..., 20]
        // P90 index = ceil(19 * 0.9) = ceil(17.1) = 18 → sorted[18] = 19
        let mut depths: Vec<usize> = (1..=20).collect();
        assert_eq!(super::percentile_90(&mut depths), 19);
    }
}
