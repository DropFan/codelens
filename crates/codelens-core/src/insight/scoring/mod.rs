//! Scoring model abstraction for code health analysis.

pub mod default;
pub mod legacy;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::analyzer::stats::FileStats;
use crate::insight::Grade;

/// Complete health-scoring pipeline selected by users and configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthModelVersion {
    #[default]
    V1,
    V2,
}

impl fmt::Display for HealthModelVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
        })
    }
}

impl FromStr for HealthModelVersion {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "v1" => Ok(Self::V1),
            "v2" => Ok(Self::V2),
            _ => Err(format!(
                "invalid value '{value}' for 'health_model': expected v1 or v2"
            )),
        }
    }
}

/// Internal aggregation behavior associated with a scoring model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HealthScoringFlow {
    #[default]
    Legacy,
    FileFirstV2,
}

/// Create a built-in model for one complete pipeline and measurement scope.
pub fn scoring_model_for(
    version: HealthModelVersion,
    duplication_measured: bool,
) -> Box<dyn ScoringModel> {
    match (version, duplication_measured) {
        (HealthModelVersion::V1, true) => Box::new(legacy::LegacyModel::new()),
        (HealthModelVersion::V1, false) => Box::new(legacy::LegacyModel::without_duplication()),
        (HealthModelVersion::V2, true) => Box::new(default::DefaultModel::new()),
        (HealthModelVersion::V2, false) => Box::new(default::DefaultModel::without_duplication()),
    }
}

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
    /// For multiple files: P90 of all files' maximum control-flow depths.
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
        Self::from_file_with_mode(file, false)
    }

    /// Build metrics with the analyzer behavior used by health model v1.
    /// Old snapshots have no explicit legacy fields, so `legacy_metrics`
    /// deliberately falls back to their primary fields.
    pub fn from_file_legacy(file: &FileStats) -> Self {
        Self::from_file_with_mode(file, true)
    }

    fn from_file_with_mode(file: &FileStats, legacy: bool) -> Self {
        let (functions, cyclomatic, depth, avg_func_lines) = complexity_values(file, legacy);
        let avg_cyclomatic = if functions > 0 {
            cyclomatic as f64 / functions as f64
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
            avg_func_lines,
            comment_ratio,
            depth,
            avg_file_lines: file.lines.total as f64,
            total_files: 1,
            duplication_ratio,
        }
    }

    pub fn from_file_refs(files: &[&FileStats]) -> Self {
        Self::from_file_refs_with_mode(files, false)
    }

    /// Aggregate metrics with v1 analyzer inputs.
    pub fn from_file_refs_legacy(files: &[&FileStats]) -> Self {
        Self::from_file_refs_with_mode(files, true)
    }

    fn from_file_refs_with_mode(files: &[&FileStats], legacy: bool) -> Self {
        if files.is_empty() {
            return Self::default();
        }
        let complexity: Vec<(usize, usize, usize, f64)> = files
            .iter()
            .map(|file| complexity_values(file, legacy))
            .collect();
        let total_functions: usize = complexity.iter().map(|values| values.0).sum();
        let total_cyclomatic: usize = complexity.iter().map(|values| values.1).sum();
        let total_code: usize = files.iter().map(|f| f.lines.code).sum();
        let total_comment: usize = files.iter().map(|f| f.lines.comment).sum();
        let total_lines: usize = files.iter().map(|f| f.lines.total).sum();

        let mut depths: Vec<usize> = complexity.iter().map(|values| values.2).collect();
        let depth = percentile_90(&mut depths);

        let avg_cyclomatic = if total_functions > 0 {
            total_cyclomatic as f64 / total_functions as f64
        } else {
            0.0
        };

        let total_function_lines: f64 = files
            .iter()
            .zip(&complexity)
            .filter(|(_, values)| values.0 > 0)
            .map(|(_, values)| values.3 * values.0 as f64)
            .sum();
        let avg_func_lines = if total_functions > 0 {
            total_function_lines / total_functions as f64
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

    pub fn from_files_legacy(files: &[FileStats]) -> Self {
        let refs: Vec<&FileStats> = files.iter().collect();
        Self::from_file_refs_legacy(&refs)
    }
}

fn complexity_values(file: &FileStats, legacy: bool) -> (usize, usize, usize, f64) {
    if legacy {
        file.complexity.legacy_metrics()
    } else {
        (
            file.complexity.functions,
            file.complexity.cyclomatic,
            file.complexity.max_depth,
            file.complexity.avg_func_lines,
        )
    }
}

/// Compute the historical P90 percentile of a mutable slice (sorts in place).
/// For a single element, returns that element. For empty, returns 0.
fn percentile_90(values: &mut [usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() as f64 - 1.0) * 0.9).ceil() as usize;
    values[index.min(values.len() - 1)]
}

pub trait ScoringModel: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> &[DimensionWeight];
    fn score_dimension(&self, dimension: HealthDimension, metrics: &RawMetrics) -> f64;

    /// Existing third-party models retain the historical aggregation flow.
    fn health_scoring_flow(&self) -> HealthScoringFlow {
        HealthScoringFlow::Legacy
    }

    fn grade(&self, score: f64) -> Grade {
        let score = if score.is_finite() { score + 1e-9 } else { 0.0 };
        if score >= 90.0 {
            Grade::A
        } else if score >= 80.0 {
            Grade::B
        } else if score >= 70.0 {
            Grade::C
        } else if score >= 60.0 {
            Grade::D
        } else {
            Grade::F
        }
    }

    fn total_score(&self, metrics: &RawMetrics) -> f64 {
        if metrics.total_files == 0 {
            return 0.0;
        }
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
                functions_measured: true,
                control_flow_measured: true,
                functions: 4,
                cyclomatic: 12,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 20.0,
                ..Complexity::default()
            },
        };
        let metrics = RawMetrics::from_file(&file);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        assert!((metrics.comment_ratio - 0.125).abs() < 0.01);
        assert_eq!(metrics.depth, 3);
    }

    #[test]
    fn legacy_raw_metrics_use_frozen_analyzer_values() {
        let file = FileStats {
            path: PathBuf::from("legacy.ts"),
            language: "TypeScript".to_string(),
            lines: LineStats {
                total: 120,
                code: 100,
                comment: 10,
                blank: 10,
            },
            size: 2_000,
            duplicate_lines: 0,
            complexity: Complexity {
                functions: 10,
                cyclomatic: 30,
                max_depth: 2,
                avg_func_lines: 12.0,
                legacy_functions: Some(2),
                legacy_cyclomatic: Some(12),
                legacy_max_depth: Some(7),
                legacy_avg_func_lines: Some(40.0),
                ..Complexity::default()
            },
        };

        let current = RawMetrics::from_file(&file);
        let legacy = RawMetrics::from_file_legacy(&file);

        assert_eq!(current.avg_cyclomatic, 3.0);
        assert_eq!(current.avg_func_lines, 12.0);
        assert_eq!(current.depth, 2);
        assert_eq!(legacy.avg_cyclomatic, 6.0);
        assert_eq!(legacy.avg_func_lines, 40.0);
        assert_eq!(legacy.depth, 7);
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
                functions_measured: true,
                control_flow_measured: true,
                functions: 10,
                cyclomatic: 20,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 20.0,
                ..Complexity::default()
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
                    functions_measured: true,
                    control_flow_measured: true,
                    functions: 4,
                    cyclomatic: 12,
                    cognitive: 0,
                    max_depth: 3,
                    avg_func_lines: 20.0,
                    ..Complexity::default()
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
                    functions_measured: true,
                    control_flow_measured: true,
                    functions: 2,
                    cyclomatic: 6,
                    cognitive: 0,
                    max_depth: 5,
                    avg_func_lines: 5.0,
                    ..Complexity::default()
                },
            },
        ];
        let metrics = RawMetrics::from_files(&files);
        assert_eq!(metrics.total_files, 2);
        assert!((metrics.avg_cyclomatic - 3.0).abs() < 0.01);
        // P90 of [3, 5] = 5 (only 2 elements, P90 picks the higher)
        assert_eq!(metrics.depth, 5);
        assert!((metrics.avg_func_lines - 15.0).abs() < 0.01);
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
        // Preserve the historical percentile definition used by v1.
        let mut depths = vec![3; 19];
        depths.push(15);
        assert_eq!(super::percentile_90(&mut depths), 3);
    }

    #[test]
    fn test_percentile_90_gradual() {
        // depths: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        // Historical index: ceil((10 - 1) * .9) = 9 → sorted[9] = 10.
        let mut depths: Vec<usize> = (1..=10).collect();
        assert_eq!(super::percentile_90(&mut depths), 10);

        // Historical index: ceil((20 - 1) * .9) = 18 → sorted[18] = 19.
        let mut depths: Vec<usize> = (1..=20).collect();
        assert_eq!(super::percentile_90(&mut depths), 19);
    }

    #[test]
    fn health_model_version_is_strict_and_defaults_to_v1() {
        assert_eq!(HealthModelVersion::default(), HealthModelVersion::V1);
        assert_eq!("v1".parse(), Ok(HealthModelVersion::V1));
        assert_eq!("v2".parse(), Ok(HealthModelVersion::V2));
        assert!("default".parse::<HealthModelVersion>().is_err());
    }

    #[test]
    fn model_factory_preserves_versioned_names_and_flows() {
        let v1 = scoring_model_for(HealthModelVersion::V1, true);
        let v1_no_dup = scoring_model_for(HealthModelVersion::V1, false);
        let v2 = scoring_model_for(HealthModelVersion::V2, true);
        let v2_no_dup = scoring_model_for(HealthModelVersion::V2, false);

        assert_eq!(v1.name(), "default");
        assert_eq!(v1_no_dup.name(), "default-no-dup");
        assert_eq!(v2.name(), "default-v2");
        assert_eq!(v2_no_dup.name(), "default-v2-no-dup");
        assert_eq!(v1.health_scoring_flow(), HealthScoringFlow::Legacy);
        assert_eq!(v2.health_scoring_flow(), HealthScoringFlow::FileFirstV2);
    }

    #[test]
    fn test_empty_metrics_score_zero() {
        let model = crate::insight::scoring::default::DefaultModel::new();
        assert_eq!(model.total_score(&RawMetrics::default()), 0.0);
    }
}
