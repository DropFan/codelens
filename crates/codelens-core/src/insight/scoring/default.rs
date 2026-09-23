//! Default scoring model for general-purpose projects.

use super::{DimensionWeight, HealthDimension, HealthScoringFlow, RawMetrics, ScoringModel};

/// Duplication must stay last: `without_duplication` drops it by
/// slicing off the tail (checked by test).
const ALL_DIMENSIONS: &[DimensionWeight] = &[
    DimensionWeight {
        dimension: HealthDimension::Complexity,
        weight: 0.30,
    },
    DimensionWeight {
        dimension: HealthDimension::FuncSize,
        weight: 0.20,
    },
    DimensionWeight {
        dimension: HealthDimension::CommentRatio,
        weight: 0.05,
    },
    DimensionWeight {
        dimension: HealthDimension::FileSize,
        weight: 0.20,
    },
    DimensionWeight {
        dimension: HealthDimension::NestingDepth,
        weight: 0.05,
    },
    DimensionWeight {
        dimension: HealthDimension::Duplication,
        weight: 0.20,
    },
];

pub struct DefaultModel {
    dimensions: &'static [DimensionWeight],
    name: &'static str,
}

impl DefaultModel {
    pub fn new() -> Self {
        Self {
            dimensions: ALL_DIMENSIONS,
            name: "default-v2",
        }
    }

    /// Model for analyses where line duplication was not collected
    /// (--no-dup-scan or a snapshot without duplication data): the
    /// Duplication dimension is excluded rather than scored as clean.
    /// The remaining weights are renormalized by the weight sum, both
    /// in `total_score` and in the serialized per-dimension weights.
    /// The distinct name lets machine-read output (JSON, openmetrics)
    /// tell the reduced scoring basis apart from the full model.
    pub fn without_duplication() -> Self {
        Self {
            dimensions: &ALL_DIMENSIONS[..ALL_DIMENSIONS.len() - 1],
            name: "default-v2-no-dup",
        }
    }
}

impl Default for DefaultModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringModel for DefaultModel {
    fn name(&self) -> &str {
        self.name
    }

    fn dimensions(&self) -> &[DimensionWeight] {
        self.dimensions
    }

    fn health_scoring_flow(&self) -> HealthScoringFlow {
        HealthScoringFlow::FileFirstV2
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

    fn total_score(&self, metrics: &RawMetrics) -> f64 {
        if metrics.total_files == 0 {
            return 0.0;
        }
        let total_weight: f64 = self
            .dimensions
            .iter()
            .map(|dimension| dimension.weight)
            .sum();
        if total_weight == 0.0 {
            return 0.0;
        }
        let scores: Vec<(HealthDimension, f64, f64)> = self
            .dimensions
            .iter()
            .map(|dimension| {
                (
                    dimension.dimension,
                    self.score_dimension(dimension.dimension, metrics),
                    dimension.weight,
                )
            })
            .collect();
        let base = scores
            .iter()
            .map(|(_, score, weight)| score * weight)
            .sum::<f64>()
            / total_weight;
        let core_floor = scores
            .iter()
            .filter(|(dimension, _, _)| {
                matches!(
                    dimension,
                    HealthDimension::Complexity
                        | HealthDimension::FuncSize
                        | HealthDimension::FileSize
                        | HealthDimension::Duplication
                )
            })
            .map(|(_, score, _)| *score)
            .min_by(f64::total_cmp)
            .unwrap_or(base);
        (0.85 * base + 0.15 * core_floor).clamp(0.0, 100.0)
    }
}

fn interpolate(value: f64, breakpoints: &[(f64, f64)]) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
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
            (15.0, 35.0),
            (25.0, 10.0),
            (40.0, 1.0),
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
            (80.0, 35.0),
            (150.0, 10.0),
            (300.0, 1.0),
        ],
    )
}

fn score_comment_ratio(ratio: f64) -> f64 {
    interpolate(
        ratio,
        &[
            (0.0, 60.0),
            (0.03, 80.0),
            (0.05, 100.0),
            (0.30, 100.0),
            (0.50, 80.0),
            (0.80, 60.0),
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
            (1000.0, 35.0),
            (2000.0, 10.0),
            (4000.0, 1.0),
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
            (0.25, 100.0),
            (0.35, 80.0),
            (0.45, 60.0),
            (0.60, 30.0),
            (0.75, 10.0),
            (0.90, 1.0),
        ],
    )
}

fn score_nesting(depth: usize) -> f64 {
    interpolate(
        depth as f64,
        &[
            (0.0, 100.0),
            (2.0, 100.0),
            (3.0, 85.0),
            (4.0, 65.0),
            (6.0, 35.0),
            (8.0, 10.0),
            (12.0, 1.0),
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
    fn test_interpolate_non_finite_is_worst_score() {
        let bp = &[(0.0, 100.0), (10.0, 20.0)];
        assert_eq!(interpolate(f64::NAN, bp), 1.0);
        assert_eq!(interpolate(f64::INFINITY, bp), 1.0);
        assert_eq!(interpolate(f64::NEG_INFINITY, bp), 1.0);
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
        assert!((score_comment_ratio(0.0) - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_score_comment_ratio_too_many() {
        assert!((score_comment_ratio(0.80) - 60.0).abs() < 0.01);
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
        assert_eq!(DefaultModel::new().name(), "default-v2");
        // The reduced-scope variant must be distinguishable in
        // machine-read output (JSON "model" field, openmetrics label),
        // or consumers cannot tell the scoring basis changed.
        assert_eq!(
            DefaultModel::without_duplication().name(),
            "default-v2-no-dup"
        );
    }

    #[test]
    fn test_default_model_weights_sum_to_one() {
        let model = DefaultModel::new();
        let total: f64 = model.dimensions().iter().map(|d| d.weight).sum();
        assert!((total - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_without_duplication_excludes_only_that_dimension() {
        let model = DefaultModel::without_duplication();
        assert_eq!(model.dimensions().len(), ALL_DIMENSIONS.len() - 1);
        assert!(model
            .dimensions()
            .iter()
            .all(|d| d.dimension != HealthDimension::Duplication));
    }

    #[test]
    fn test_without_duplication_renormalizes_weights() {
        // Perfect metrics except a terrible duplication ratio: the full
        // model is dragged down by the Duplication dimension, while the
        // unmeasured model must land on a full 100 — not the 80 that an
        // un-renormalized weight sum would produce.
        let metrics = RawMetrics {
            avg_cyclomatic: 2.0,
            avg_func_lines: 10.0,
            comment_ratio: 0.15,
            depth: 2,
            avg_file_lines: 100.0,
            total_files: 10,
            duplication_ratio: 0.9,
        };
        let full = DefaultModel::new().total_score(&metrics);
        let unmeasured = DefaultModel::without_duplication().total_score(&metrics);
        assert!(full < 95.0, "full model must feel the duplication penalty");
        assert!(
            (unmeasured - 100.0).abs() < 0.001,
            "unmeasured model must renormalize to 100, got {unmeasured}"
        );
    }

    #[test]
    fn test_severe_weighted_dimension_cannot_hide_in_an_a_grade() {
        let metrics = RawMetrics {
            avg_cyclomatic: 2.0,
            avg_func_lines: 10.0,
            comment_ratio: 0.15,
            depth: 2,
            avg_file_lines: 100.0,
            total_files: 1,
            duplication_ratio: 0.90,
        };

        let score = DefaultModel::new().total_score(&metrics);
        assert!(
            score < 70.0,
            "severe duplication must be visible, got {score}"
        );
    }
}
