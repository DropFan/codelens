//! Default scoring model for general-purpose projects.

use super::{DimensionWeight, HealthDimension, RawMetrics, ScoringModel};

pub struct DefaultModel;

impl DefaultModel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringModel for DefaultModel {
    fn name(&self) -> &str {
        "default"
    }

    fn dimensions(&self) -> &[DimensionWeight] {
        &[
            DimensionWeight {
                dimension: HealthDimension::Complexity,
                weight: 0.25,
            },
            DimensionWeight {
                dimension: HealthDimension::FuncSize,
                weight: 0.20,
            },
            DimensionWeight {
                dimension: HealthDimension::CommentRatio,
                weight: 0.10,
            },
            DimensionWeight {
                dimension: HealthDimension::FileSize,
                weight: 0.20,
            },
            DimensionWeight {
                dimension: HealthDimension::NestingDepth,
                weight: 0.15,
            },
            DimensionWeight {
                dimension: HealthDimension::Duplication,
                weight: 0.10,
            },
        ]
    }

    fn score_dimension(&self, dimension: HealthDimension, metrics: &RawMetrics) -> f64 {
        match dimension {
            HealthDimension::Complexity => score_complexity(metrics.avg_cyclomatic),
            HealthDimension::FuncSize => score_func_size(metrics.avg_func_lines),
            HealthDimension::CommentRatio => score_comment_ratio(metrics.comment_ratio),
            HealthDimension::FileSize => score_file_size(metrics.avg_file_lines),
            HealthDimension::NestingDepth => score_nesting(metrics.depth),
            HealthDimension::Duplication => score_duplication(metrics.duplication_ratio),
        }
    }
}

fn interpolate(value: f64, breakpoints: &[(f64, f64)]) -> f64 {
    if breakpoints.is_empty() {
        return 50.0;
    }
    if value <= breakpoints[0].0 {
        return breakpoints[0].1;
    }
    if value >= breakpoints[breakpoints.len() - 1].0 {
        return breakpoints[breakpoints.len() - 1].1;
    }
    for window in breakpoints.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if value >= x0 && value <= x1 {
            let t = (value - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    50.0
}

fn score_complexity(avg_cc: f64) -> f64 {
    interpolate(
        avg_cc,
        &[
            (0.0, 100.0),
            (3.0, 100.0),
            (6.0, 80.0),
            (10.0, 60.0),
            (15.0, 40.0),
            (25.0, 20.0),
        ],
    )
}

fn score_func_size(avg_lines: f64) -> f64 {
    interpolate(
        avg_lines,
        &[
            (0.0, 100.0),
            (15.0, 100.0),
            (30.0, 80.0),
            (50.0, 60.0),
            (80.0, 40.0),
            (150.0, 20.0),
        ],
    )
}

fn score_comment_ratio(ratio: f64) -> f64 {
    interpolate(
        ratio,
        &[
            (0.0, 40.0),
            (0.03, 70.0),
            (0.05, 100.0),
            (0.30, 100.0),
            (0.50, 70.0),
            (0.80, 40.0),
        ],
    )
}

fn score_file_size(avg_lines: f64) -> f64 {
    interpolate(
        avg_lines,
        &[
            (0.0, 100.0),
            (200.0, 100.0),
            (400.0, 80.0),
            (600.0, 60.0),
            (1000.0, 40.0),
            (2000.0, 20.0),
        ],
    )
}

/// Duplicated line instances / non-blank lines. Structural noise
/// (brace-only lines, repeated imports) duplicates in every codebase, so
/// the curve treats a moderate baseline as healthy and only punishes
/// clearly copy-paste-heavy ratios.
fn score_duplication(ratio: f64) -> f64 {
    interpolate(
        ratio,
        &[
            (0.00, 100.0),
            (0.30, 100.0),
            (0.40, 80.0),
            (0.50, 60.0),
            (0.60, 40.0),
            (0.75, 20.0),
        ],
    )
}

fn score_nesting(depth: usize) -> f64 {
    interpolate(
        depth as f64,
        &[
            (0.0, 100.0),
            (3.0, 100.0),
            (4.0, 80.0),
            (6.0, 60.0),
            (8.0, 40.0),
            (12.0, 20.0),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_at_breakpoint() {
        let bp = &[(0.0, 100.0), (10.0, 0.0)];
        assert!((interpolate(0.0, bp) - 100.0).abs() < 0.01);
        assert!((interpolate(10.0, bp) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_interpolate_midpoint() {
        let bp = &[(0.0, 100.0), (10.0, 0.0)];
        assert!((interpolate(5.0, bp) - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_interpolate_below_min() {
        let bp = &[(5.0, 80.0), (10.0, 40.0)];
        assert!((interpolate(2.0, bp) - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_interpolate_above_max() {
        let bp = &[(0.0, 100.0), (10.0, 20.0)];
        assert!((interpolate(999.0, bp) - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_score_complexity_excellent() {
        assert!((score_complexity(2.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_score_complexity_poor() {
        let score = score_complexity(20.0);
        assert!(score < 40.0);
        assert!(score > 20.0);
    }

    #[test]
    fn test_score_comment_ratio_sweet_spot() {
        assert!((score_comment_ratio(0.10) - 100.0).abs() < 0.01);
        assert!((score_comment_ratio(0.20) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_score_comment_ratio_too_few() {
        assert!(score_comment_ratio(0.0) < 50.0);
    }

    #[test]
    fn test_score_comment_ratio_too_many() {
        assert!(score_comment_ratio(0.80) < 50.0);
    }

    #[test]
    fn test_default_model_total_score() {
        let model = DefaultModel::new();
        let metrics = RawMetrics {
            avg_cyclomatic: 2.0,
            avg_func_lines: 10.0,
            comment_ratio: 0.15,
            depth: 2,
            avg_file_lines: 100.0,
            total_files: 10,
            duplication_ratio: 0.0,
        };
        let score = model.total_score(&metrics);
        assert!(
            score >= 90.0,
            "excellent metrics should give A grade, got {score}"
        );
    }

    #[test]
    fn test_default_model_poor_score() {
        let model = DefaultModel::new();
        let metrics = RawMetrics {
            avg_cyclomatic: 20.0,
            avg_func_lines: 100.0,
            comment_ratio: 0.0,
            depth: 10,
            avg_file_lines: 1500.0,
            total_files: 5,
            duplication_ratio: 0.0,
        };
        let score = model.total_score(&metrics);
        assert!(
            score < 50.0,
            "poor metrics should give F grade, got {score}"
        );
    }

    #[test]
    fn test_default_model_grade() {
        let model = DefaultModel::new();
        assert_eq!(model.grade(95.0), crate::insight::Grade::A);
        assert_eq!(model.grade(85.0), crate::insight::Grade::B);
        assert_eq!(model.grade(75.0), crate::insight::Grade::C);
        assert_eq!(model.grade(65.0), crate::insight::Grade::D);
        assert_eq!(model.grade(50.0), crate::insight::Grade::F);
    }

    #[test]
    fn test_default_model_name() {
        assert_eq!(DefaultModel::new().name(), "default");
    }

    #[test]
    fn test_default_model_weights_sum_to_one() {
        let model = DefaultModel::new();
        let total: f64 = model.dimensions().iter().map(|d| d.weight).sum();
        assert!((total - 1.0).abs() < 0.001);
    }
}
