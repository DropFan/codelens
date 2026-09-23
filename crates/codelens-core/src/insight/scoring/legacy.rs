//! Frozen v1 health-scoring model.
//!
//! This module intentionally keeps the historical weights and curves so
//! users can reproduce existing gates while migrating to the v2 pipeline.

use super::{DimensionWeight, HealthDimension, RawMetrics, ScoringModel};

const ALL_DIMENSIONS: &[DimensionWeight] = &[
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
];

pub struct LegacyModel {
    dimensions: &'static [DimensionWeight],
    name: &'static str,
}

impl LegacyModel {
    pub fn new() -> Self {
        Self {
            dimensions: ALL_DIMENSIONS,
            name: "default",
        }
    }

    pub fn without_duplication() -> Self {
        Self {
            dimensions: &ALL_DIMENSIONS[..ALL_DIMENSIONS.len() - 1],
            name: "default-no-dup",
        }
    }
}

impl Default for LegacyModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringModel for LegacyModel {
    fn name(&self) -> &str {
        self.name
    }

    fn dimensions(&self) -> &[DimensionWeight] {
        self.dimensions
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

fn score_complexity(value: f64) -> f64 {
    interpolate(
        value,
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

fn score_func_size(value: f64) -> f64 {
    interpolate(
        value,
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

fn score_comment_ratio(value: f64) -> f64 {
    interpolate(
        value,
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

fn score_file_size(value: f64) -> f64 {
    interpolate(
        value,
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

fn score_nesting(value: usize) -> f64 {
    interpolate(
        value as f64,
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

fn score_duplication(value: f64) -> f64 {
    interpolate(
        value,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_weights_are_frozen() {
        let model = LegacyModel::new();
        assert_eq!(model.name(), "default");
        assert!((model.dimensions().iter().map(|d| d.weight).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(LegacyModel::without_duplication().name(), "default-no-dup");
    }

    #[test]
    fn historical_curves_remain_distinct_from_v2() {
        let metrics = RawMetrics {
            avg_cyclomatic: 15.0,
            avg_func_lines: 80.0,
            comment_ratio: 0.0,
            depth: 8,
            avg_file_lines: 1_000.0,
            total_files: 1,
            duplication_ratio: 0.60,
        };
        assert!((LegacyModel::new().total_score(&metrics) - 40.0).abs() < 0.01);
    }
}
