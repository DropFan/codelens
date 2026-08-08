//! Pluggable cost estimation models.
//!
//! Four built-in models:
//! - **COCOMO Basic** — classic Boehm 1981, three project types
//! - **COCOMO II** — modern 2000 calibration, scale factors + cost drivers
//! - **Putnam/SLIM** — Rayleigh-curve manpower model
//! - **LOCOMO** — LLM Output Cost Model (AI-era code generation cost)

use serde::Serialize;

use crate::analyzer::stats::Summary;

// ── Shared types ────────────────────────────────────────

/// Code metrics extracted per-language or globally.
#[derive(Debug, Clone, Default)]
pub struct CodeMetrics {
    pub code_lines: usize,
    pub cyclomatic_complexity: usize,
}

/// Shared cost parameters (salary-based models).
#[derive(Debug, Clone, Serialize)]
pub struct CostConfig {
    /// Average annual salary in USD.
    pub average_wage: f64,
    /// Overhead multiplier (facilities, management, etc.).
    pub overhead: f64,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            average_wage: 56_286.0,
            overhead: 2.4,
        }
    }
}

/// COCOMO project complexity type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ProjectType {
    #[default]
    Organic,
    SemiDetached,
    Embedded,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Organic => write!(f, "organic"),
            Self::SemiDetached => write!(f, "semi-detached"),
            Self::Embedded => write!(f, "embedded"),
        }
    }
}

// ── Result types ────────────────────────────────────────

/// Per-language estimation breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageEstimation {
    pub language: String,
    pub code_lines: usize,
    pub effort_months: f64,
    pub cost: f64,
}

/// Complete estimation report.
#[derive(Debug, Clone, Serialize)]
pub struct EstimationReport {
    pub model: String,
    pub total_sloc: usize,
    pub effort_months: f64,
    pub schedule_months: f64,
    pub people_required: f64,
    pub estimated_cost: f64,
    pub by_language: Vec<LanguageEstimation>,
    /// Model-specific parameters for display.
    pub params: Vec<(String, String)>,
}

// ── Trait ────────────────────────────────────────────────

/// Pluggable estimation model.
pub trait EstimationModel: Send + Sync {
    /// Model display name.
    fn name(&self) -> &str;

    /// Key parameters for display in report footer.
    fn display_params(&self) -> Vec<(String, String)>;

    /// Estimate effort in person-months.
    fn estimate_effort(&self, metrics: &CodeMetrics) -> f64;

    /// Estimate schedule in months.
    fn estimate_schedule(&self, effort_months: f64, metrics: &CodeMetrics) -> f64;

    /// Estimate cost in USD. Default: salary-based.
    fn estimate_cost(
        &self,
        effort_months: f64,
        metrics: &CodeMetrics,
        cost_config: &CostConfig,
    ) -> f64 {
        let _ = metrics;
        effort_months * (cost_config.average_wage / 12.0) * cost_config.overhead
    }

    /// Estimate people required. Default: effort / schedule.
    fn estimate_people(&self, effort_months: f64, schedule_months: f64) -> f64 {
        if schedule_months > 0.0 {
            effort_months / schedule_months
        } else {
            0.0
        }
    }
}

// ── Public API ──────────────────────────────────────────

/// Generate estimation report from analysis summary.
pub fn estimate(
    summary: &Summary,
    model: &dyn EstimationModel,
    cost_config: &CostConfig,
) -> EstimationReport {
    let global_metrics = CodeMetrics {
        code_lines: summary.lines.code,
        cyclomatic_complexity: summary.complexity.cyclomatic,
    };

    // Per-language breakdown
    let mut by_language: Vec<LanguageEstimation> = summary
        .by_language
        .iter()
        .filter(|(_, ls)| ls.lines.code > 0)
        .map(|(lang, ls)| {
            let metrics = CodeMetrics {
                code_lines: ls.lines.code,
                cyclomatic_complexity: ls.complexity.cyclomatic,
            };
            let effort = model.estimate_effort(&metrics);
            let cost = model.estimate_cost(effort, &metrics, cost_config);
            LanguageEstimation {
                language: lang.clone(),
                code_lines: ls.lines.code,
                effort_months: effort,
                cost,
            }
        })
        .collect();

    by_language.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Global estimation (non-linear: estimate(total) ≠ Σ estimate(part))
    let effort_months = model.estimate_effort(&global_metrics);
    let schedule_months = model.estimate_schedule(effort_months, &global_metrics);
    let people_required = model.estimate_people(effort_months, schedule_months);
    let estimated_cost = model.estimate_cost(effort_months, &global_metrics, cost_config);

    EstimationReport {
        model: model.name().to_string(),
        total_sloc: summary.lines.code,
        effort_months,
        schedule_months,
        people_required,
        estimated_cost,
        by_language,
        params: model.display_params(),
    }
}

/// Multi-model comparison report.
#[derive(Debug, Clone, Serialize)]
pub struct EstimationComparison {
    pub total_sloc: usize,
    pub reports: Vec<EstimationReport>,
}

/// Run all given models and produce a comparison report.
pub fn estimate_all(
    summary: &Summary,
    models: &[&dyn EstimationModel],
    cost_config: &CostConfig,
) -> EstimationComparison {
    let reports = models
        .iter()
        .map(|m| estimate(summary, *m, cost_config))
        .collect();
    EstimationComparison {
        total_sloc: summary.lines.code,
        reports,
    }
}

// ════════════════════════════════════════════════════════
// Model 1: COCOMO I Basic
// ════════════════════════════════════════════════════════

/// COCOMO I Basic model (Boehm 1981).
///
/// Coefficients [a, b, c, d] per project type:
///   Organic:       [2.4, 1.05, 2.5, 0.38]
///   Semi-Detached: [3.0, 1.12, 2.5, 0.35]
///   Embedded:      [3.6, 1.20, 2.5, 0.32]
pub struct CocomoBasicModel {
    pub project_type: ProjectType,
    pub eaf: f64,
}

impl Default for CocomoBasicModel {
    fn default() -> Self {
        Self {
            project_type: ProjectType::Organic,
            eaf: 1.0,
        }
    }
}

fn cocomo_basic_coefficients(pt: ProjectType) -> [f64; 4] {
    match pt {
        ProjectType::Organic => [2.4, 1.05, 2.5, 0.38],
        ProjectType::SemiDetached => [3.0, 1.12, 2.5, 0.35],
        ProjectType::Embedded => [3.6, 1.20, 2.5, 0.32],
    }
}

impl EstimationModel for CocomoBasicModel {
    fn name(&self) -> &str {
        "COCOMO Basic"
    }

    fn display_params(&self) -> Vec<(String, String)> {
        vec![
            ("Project Type".into(), self.project_type.to_string()),
            ("EAF".into(), format!("{:.2}", self.eaf)),
        ]
    }

    fn estimate_effort(&self, metrics: &CodeMetrics) -> f64 {
        let [a, b, _, _] = cocomo_basic_coefficients(self.project_type);
        a * (metrics.code_lines as f64 / 1000.0).powf(b) * self.eaf
    }

    fn estimate_schedule(&self, effort_months: f64, _metrics: &CodeMetrics) -> f64 {
        let [_, _, c, d] = cocomo_basic_coefficients(self.project_type);
        c * effort_months.powf(d)
    }
}

// ════════════════════════════════════════════════════════
// Model 2: COCOMO II Post-Architecture
// ════════════════════════════════════════════════════════

/// Nominal values for the 5 COCOMO II scale factors.
pub const COCOMO2_SF_NOMINAL: [f64; 5] = [3.72, 3.04, 4.24, 3.29, 4.68];

/// COCOMO II Post-Architecture model (Boehm 2000).
///
/// Effort: PM = A × EAF × (KSLOC)^E
///   where E = B + 0.01 × Σ SF_i
///
/// Schedule: TDEV = C × (PM)^F
///   where F = D + 0.2 × (E − B)
pub struct CocomoIIModel {
    /// Multiplicative constant A (default 2.94).
    pub a: f64,
    /// Base exponent B (default 0.91).
    pub b: f64,
    /// Schedule constant C (default 3.67).
    pub c: f64,
    /// Schedule base exponent D (default 0.28).
    pub d: f64,
    /// 5 scale factors [PREC, FLEX, RESL, TEAM, PMAT].
    pub scale_factors: [f64; 5],
    /// Product of all cost driver multipliers (default 1.0 = all nominal).
    pub eaf: f64,
}

impl Default for CocomoIIModel {
    fn default() -> Self {
        Self {
            a: 2.94,
            b: 0.91,
            c: 3.67,
            d: 0.28,
            scale_factors: COCOMO2_SF_NOMINAL,
            eaf: 1.0,
        }
    }
}

impl CocomoIIModel {
    fn exponent_e(&self) -> f64 {
        self.b + 0.01 * self.scale_factors.iter().sum::<f64>()
    }

    fn exponent_f(&self) -> f64 {
        self.d + 0.2 * (self.exponent_e() - self.b)
    }
}

impl EstimationModel for CocomoIIModel {
    fn name(&self) -> &str {
        "COCOMO II"
    }

    fn display_params(&self) -> Vec<(String, String)> {
        let sf_sum: f64 = self.scale_factors.iter().sum();
        vec![
            ("A".into(), format!("{:.2}", self.a)),
            ("E (exponent)".into(), format!("{:.4}", self.exponent_e())),
            ("SF sum".into(), format!("{:.2}", sf_sum)),
            ("EAF".into(), format!("{:.2}", self.eaf)),
        ]
    }

    fn estimate_effort(&self, metrics: &CodeMetrics) -> f64 {
        let e = self.exponent_e();
        self.a * self.eaf * (metrics.code_lines as f64 / 1000.0).powf(e)
    }

    fn estimate_schedule(&self, effort_months: f64, _metrics: &CodeMetrics) -> f64 {
        let f = self.exponent_f();
        self.c * effort_months.powf(f)
    }
}

// ════════════════════════════════════════════════════════
// Model 3: Putnam / SLIM (Rayleigh curve)
// ════════════════════════════════════════════════════════

/// Putnam/SLIM model.
///
/// Software Equation: Size = Ck × E^(1/3) × T^(4/3)
/// Combined with D0 = E / T³:
///   T = (Size / (Ck × D0^(1/3)))^(3/7)   \[years\]
///   E = D0 × T³                            \[person-years\]
pub struct PutnamModel {
    /// Technology/productivity constant Ck.
    /// Typical: 2000 (poor), 8000 (good), 11000 (excellent).
    pub ck: f64,
    /// Manpower buildup index D0 = Effort / Time³.
    /// Typical: 8 (new/many interfaces), 15 (new standalone), 27 (rebuild).
    pub d0: f64,
}

impl Default for PutnamModel {
    fn default() -> Self {
        Self {
            ck: 8000.0,
            d0: 15.0,
        }
    }
}

impl EstimationModel for PutnamModel {
    fn name(&self) -> &str {
        "Putnam (SLIM)"
    }

    fn display_params(&self) -> Vec<(String, String)> {
        vec![
            ("Ck (productivity)".into(), format!("{:.0}", self.ck)),
            ("D0 (buildup index)".into(), format!("{:.1}", self.d0)),
        ]
    }

    fn estimate_effort(&self, metrics: &CodeMetrics) -> f64 {
        if metrics.code_lines == 0 {
            return 0.0;
        }
        let size = metrics.code_lines as f64;
        let t_years = (size / (self.ck * self.d0.powf(1.0 / 3.0))).powf(3.0 / 7.0);
        let effort_person_years = self.d0 * t_years.powi(3);
        effort_person_years * 12.0
    }

    fn estimate_schedule(&self, _effort_months: f64, metrics: &CodeMetrics) -> f64 {
        if metrics.code_lines == 0 {
            return 0.0;
        }
        let size = metrics.code_lines as f64;
        let t_years = (size / (self.ck * self.d0.powf(1.0 / 3.0))).powf(3.0 / 7.0);
        t_years * 12.0
    }
}

// ════════════════════════════════════════════════════════
// Model 4: LOCOMO (LLM Output Cost Model)
// ════════════════════════════════════════════════════════

/// LOCOMO — estimates cost/time to regenerate code with an LLM.
///
/// Per scc's LOCOMO model:
///   density = complexity / code_lines
///   cFactor = 1 + √density × complexity_weight
///   iFactor = base_iterations + √density × iteration_weight
///   outputTokens = code × tokens_per_line × iFactor
///   inputTokens  = code × input_per_line × cFactor × iFactor
///   cost = input_tokens/1M × input_price + output_tokens/1M × output_price
///   generation_seconds = output_tokens / tokens_per_second
///   review_hours = code × minutes_per_line / 60
pub struct LocomoModel {
    pub tokens_per_line: f64,
    pub input_per_line: f64,
    pub complexity_weight: f64,
    pub base_iterations: f64,
    pub iteration_weight: f64,
    pub input_price_per_m: f64,
    pub output_price_per_m: f64,
    pub tokens_per_second: f64,
    /// Human review time: minutes per line of code.
    pub minutes_per_line: f64,
}

impl Default for LocomoModel {
    fn default() -> Self {
        Self {
            tokens_per_line: 10.0,
            input_per_line: 20.0,
            complexity_weight: 5.0,
            base_iterations: 1.5,
            iteration_weight: 2.0,
            input_price_per_m: 3.0,
            output_price_per_m: 15.0,
            tokens_per_second: 50.0,
            minutes_per_line: 0.1,
        }
    }
}

impl LocomoModel {
    fn density(metrics: &CodeMetrics) -> f64 {
        if metrics.code_lines == 0 {
            return 0.0;
        }
        metrics.cyclomatic_complexity as f64 / metrics.code_lines as f64
    }

    fn token_counts(&self, metrics: &CodeMetrics) -> (f64, f64) {
        let d = Self::density(metrics);
        let c_factor = 1.0 + d.sqrt() * self.complexity_weight;
        let i_factor = self.base_iterations + d.sqrt() * self.iteration_weight;
        let output_tokens = metrics.code_lines as f64 * self.tokens_per_line * i_factor;
        let input_tokens = metrics.code_lines as f64 * self.input_per_line * c_factor * i_factor;
        (input_tokens, output_tokens)
    }
}

impl EstimationModel for LocomoModel {
    fn name(&self) -> &str {
        "LOCOMO"
    }

    fn display_params(&self) -> Vec<(String, String)> {
        vec![
            (
                "LLM Pricing".into(),
                format!(
                    "In ${:.2}/Out ${:.2} per 1M tokens",
                    self.input_price_per_m, self.output_price_per_m,
                ),
            ),
            ("TPS".into(), format!("{:.0}", self.tokens_per_second)),
            (
                "Review".into(),
                format!("{:.2} min/line", self.minutes_per_line),
            ),
        ]
    }

    /// Effort = human review hours as person-months (160 hrs/month).
    fn estimate_effort(&self, metrics: &CodeMetrics) -> f64 {
        let review_hours = metrics.code_lines as f64 * self.minutes_per_line / 60.0;
        review_hours / 160.0
    }

    /// Schedule = LLM generation time in months.
    fn estimate_schedule(&self, _effort_months: f64, metrics: &CodeMetrics) -> f64 {
        let (_, output_tokens) = self.token_counts(metrics);
        let generation_seconds = output_tokens / self.tokens_per_second;
        // Convert to months (730 hours/month * 3600 sec/hour)
        generation_seconds / (730.0 * 3600.0)
    }

    /// Cost = LLM API cost (overrides salary-based default).
    fn estimate_cost(
        &self,
        _effort_months: f64,
        metrics: &CodeMetrics,
        _cost_config: &CostConfig,
    ) -> f64 {
        let (input_tokens, output_tokens) = self.token_counts(metrics);
        (input_tokens / 1_000_000.0) * self.input_price_per_m
            + (output_tokens / 1_000_000.0) * self.output_price_per_m
    }
}

// ════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{LanguageSummary, LineStats};

    fn make_summary(code: usize, complexity: usize) -> Summary {
        let mut summary = Summary::default();
        summary.lines.code = code;
        summary.complexity.cyclomatic = complexity;
        summary.by_language.insert(
            "Rust".to_string(),
            LanguageSummary {
                files: 10,
                lines: LineStats {
                    total: code,
                    code,
                    comment: 0,
                    blank: 0,
                },
                size: 0,
                complexity: crate::analyzer::stats::Complexity {
                    cyclomatic: complexity,
                    functions: 0,
                    max_depth: 0,
                    avg_func_lines: 0.0,
                },
                tokens_est: 0,
            },
        );
        summary
    }

    fn make_multi_lang_summary() -> Summary {
        let mut summary = Summary::default();
        summary.lines.code = 12_000;
        summary.complexity.cyclomatic = 500;
        summary.by_language.insert(
            "Rust".to_string(),
            LanguageSummary {
                files: 20,
                lines: LineStats {
                    total: 10_000,
                    code: 10_000,
                    comment: 0,
                    blank: 0,
                },
                size: 0,
                complexity: crate::analyzer::stats::Complexity {
                    cyclomatic: 400,
                    functions: 0,
                    max_depth: 0,
                    avg_func_lines: 0.0,
                },
                tokens_est: 0,
            },
        );
        summary.by_language.insert(
            "Python".to_string(),
            LanguageSummary {
                files: 5,
                lines: LineStats {
                    total: 2_000,
                    code: 2_000,
                    comment: 0,
                    blank: 0,
                },
                size: 0,
                complexity: crate::analyzer::stats::Complexity {
                    cyclomatic: 100,
                    functions: 0,
                    max_depth: 0,
                    avg_func_lines: 0.0,
                },
                tokens_est: 0,
            },
        );
        summary
    }

    // ── COCOMO Basic ────────────────────────────────────

    #[test]
    fn test_cocomo_basic_effort_10k() {
        let m = CocomoBasicModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        // 2.4 * 10^1.05 = 2.4 * 11.22 = 26.93
        let effort = m.estimate_effort(&metrics);
        assert!((effort - 26.93).abs() < 0.1, "got {effort}");
    }

    #[test]
    fn test_cocomo_basic_schedule() {
        let m = CocomoBasicModel::default();
        let metrics = CodeMetrics::default();
        // 2.5 * 26.93^0.38 ≈ 8.74
        let sched = m.estimate_schedule(26.93, &metrics);
        assert!((sched - 8.74).abs() < 0.2, "got {sched}");
    }

    #[test]
    fn test_cocomo_basic_cost() {
        let m = CocomoBasicModel::default();
        let cost_config = CostConfig::default();
        let metrics = CodeMetrics::default();
        // 26.93 * (56286/12) * 2.4 = 303,222
        let cost = m.estimate_cost(26.93, &metrics, &cost_config);
        assert!((cost - 303_222.0).abs() < 500.0, "got {cost}");
    }

    #[test]
    fn test_cocomo_basic_zero_sloc() {
        let m = CocomoBasicModel::default();
        let metrics = CodeMetrics {
            code_lines: 0,
            cyclomatic_complexity: 0,
        };
        assert!(m.estimate_effort(&metrics).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cocomo_basic_eaf_doubles_effort() {
        let m1 = CocomoBasicModel::default();
        let m2 = CocomoBasicModel {
            eaf: 2.0,
            ..Default::default()
        };
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        let e1 = m1.estimate_effort(&metrics);
        let e2 = m2.estimate_effort(&metrics);
        assert!((e2 - e1 * 2.0).abs() < 0.01);
    }

    #[test]
    fn test_cocomo_basic_embedded_higher() {
        let org = CocomoBasicModel::default();
        let emb = CocomoBasicModel {
            project_type: ProjectType::Embedded,
            ..Default::default()
        };
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        assert!(emb.estimate_effort(&metrics) > org.estimate_effort(&metrics));
    }

    // ── COCOMO II ───────────────────────────────────────

    #[test]
    fn test_cocomo2_exponent_nominal() {
        let m = CocomoIIModel::default();
        // E = 0.91 + 0.01 * (3.72+3.04+4.24+3.29+4.68) = 0.91 + 0.1897 = 1.0997
        assert!((m.exponent_e() - 1.0997).abs() < 0.001);
    }

    #[test]
    fn test_cocomo2_effort_10k() {
        let m = CocomoIIModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        // 2.94 * 1.0 * 10^1.0997 = 2.94 * 12.589 = 37.01
        let effort = m.estimate_effort(&metrics);
        assert!((effort - 37.01).abs() < 0.5, "got {effort}");
    }

    #[test]
    fn test_cocomo2_schedule() {
        let m = CocomoIIModel::default();
        let metrics = CodeMetrics::default();
        // F = 0.28 + 0.2*(1.0997 - 0.91) = 0.3179
        // 3.67 * 37.01^0.3179 = 3.67 * 3.16 = 11.60
        let sched = m.estimate_schedule(37.01, &metrics);
        assert!((sched - 11.6).abs() < 0.5, "got {sched}");
    }

    #[test]
    fn test_cocomo2_higher_than_basic() {
        let basic = CocomoBasicModel::default();
        let cocomo2 = CocomoIIModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        assert!(cocomo2.estimate_effort(&metrics) > basic.estimate_effort(&metrics));
    }

    // ── Putnam ──────────────────────────────────────────

    #[test]
    fn test_putnam_effort_10k() {
        let m = PutnamModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        let effort = m.estimate_effort(&metrics);
        assert!((effort - 75.1).abs() < 1.0, "got {effort}");
    }

    #[test]
    fn test_putnam_schedule_10k() {
        let m = PutnamModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        let sched = m.estimate_schedule(0.0, &metrics);
        assert!((sched - 8.82).abs() < 0.2, "got {sched}");
    }

    #[test]
    fn test_putnam_zero_sloc() {
        let m = PutnamModel::default();
        let metrics = CodeMetrics {
            code_lines: 0,
            cyclomatic_complexity: 0,
        };
        assert!(m.estimate_effort(&metrics).abs() < f64::EPSILON);
        assert!(m.estimate_schedule(0.0, &metrics).abs() < f64::EPSILON);
    }

    #[test]
    fn test_putnam_higher_ck_less_effort() {
        let good = PutnamModel {
            ck: 8000.0,
            d0: 15.0,
        };
        let excellent = PutnamModel {
            ck: 11000.0,
            d0: 15.0,
        };
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        assert!(excellent.estimate_effort(&metrics) < good.estimate_effort(&metrics));
    }

    // ── LOCOMO ──────────────────────────────────────────

    #[test]
    fn test_locomo_density() {
        let m = CodeMetrics {
            code_lines: 1000,
            cyclomatic_complexity: 100,
        };
        assert!((LocomoModel::density(&m) - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_locomo_cost_medium_preset() {
        let m = LocomoModel::default();
        let metrics = CodeMetrics {
            code_lines: 1000,
            cyclomatic_complexity: 100,
        };
        let cost_config = CostConfig::default();
        let cost = m.estimate_cost(0.0, &metrics, &cost_config);
        assert!(cost > 0.0 && cost < 5.0, "got {cost}");
    }

    #[test]
    fn test_locomo_effort_is_review_hours() {
        let m = LocomoModel::default();
        let metrics = CodeMetrics {
            code_lines: 10_000,
            cyclomatic_complexity: 0,
        };
        // review = 10000 * 0.1 / 60 = 16.67 hours = 16.67/160 = 0.104 PM
        let effort = m.estimate_effort(&metrics);
        assert!((effort - 0.104).abs() < 0.01, "got {effort}");
    }

    #[test]
    fn test_locomo_zero_complexity() {
        let m = LocomoModel::default();
        let metrics = CodeMetrics {
            code_lines: 1000,
            cyclomatic_complexity: 0,
        };
        let cost_config = CostConfig::default();
        let cost = m.estimate_cost(0.0, &metrics, &cost_config);
        assert!(cost > 0.0, "got {cost}");
    }

    // ── Integration: estimate() ─────────────────────────

    #[test]
    fn test_estimate_report_fields() {
        let model = CocomoBasicModel::default();
        let cost_config = CostConfig::default();
        let summary = make_summary(10_000, 200);
        let report = estimate(&summary, &model, &cost_config);

        assert_eq!(report.model, "COCOMO Basic");
        assert_eq!(report.total_sloc, 10_000);
        assert!(report.effort_months > 0.0);
        assert!(report.schedule_months > 0.0);
        assert!(report.people_required > 0.0);
        assert!(report.estimated_cost > 0.0);
        assert_eq!(report.by_language.len(), 1);
    }

    #[test]
    fn test_estimate_multi_language_sorted() {
        let model = CocomoBasicModel::default();
        let cost_config = CostConfig::default();
        let summary = make_multi_lang_summary();
        let report = estimate(&summary, &model, &cost_config);

        assert_eq!(report.by_language.len(), 2);
        assert_eq!(report.by_language[0].language, "Rust");
        assert!(report.by_language[0].cost > report.by_language[1].cost);
    }

    #[test]
    fn test_estimate_empty_summary() {
        let model = CocomoBasicModel::default();
        let cost_config = CostConfig::default();
        let summary = Summary::default();
        let report = estimate(&summary, &model, &cost_config);

        assert_eq!(report.total_sloc, 0);
        assert!(report.by_language.is_empty());
    }

    #[test]
    fn test_estimate_nonlinear() {
        let model = CocomoBasicModel::default();
        let cost_config = CostConfig::default();
        let summary = make_multi_lang_summary();
        let report = estimate(&summary, &model, &cost_config);

        let sum_of_parts: f64 = report.by_language.iter().map(|l| l.effort_months).sum();
        assert!(
            (report.effort_months - sum_of_parts).abs() > 0.01,
            "global={} sum={}",
            report.effort_months,
            sum_of_parts,
        );
    }

    #[test]
    fn test_all_models_produce_report() {
        let cost_config = CostConfig::default();
        let summary = make_summary(10_000, 200);

        let models: Vec<Box<dyn EstimationModel>> = vec![
            Box::new(CocomoBasicModel::default()),
            Box::new(CocomoIIModel::default()),
            Box::new(PutnamModel::default()),
            Box::new(LocomoModel::default()),
        ];

        for model in &models {
            let report = estimate(&summary, model.as_ref(), &cost_config);
            assert!(!report.model.is_empty());
            assert_eq!(report.total_sloc, 10_000);
            assert!(
                !report.params.is_empty(),
                "model {} missing params",
                report.model
            );
        }
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(ProjectType::Organic.to_string(), "organic");
        assert_eq!(ProjectType::SemiDetached.to_string(), "semi-detached");
        assert_eq!(ProjectType::Embedded.to_string(), "embedded");
    }

    #[test]
    fn test_cost_config_defaults() {
        let c = CostConfig::default();
        assert!((c.average_wage - 56_286.0).abs() < f64::EPSILON);
        assert!((c.overhead - 2.4).abs() < f64::EPSILON);
    }
}
