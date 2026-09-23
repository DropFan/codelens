//! HTML output format with interactive charts.

use std::io::Write;

use askama::Template;

use crate::analyzer::stats::{AnalysisResult, LanguageSummary, Summary};
use crate::error::Result;
use crate::insight::Grade;

use super::format::{OutputFormat, OutputOptions, Report};

// ── Analysis (existing) ──────────────────────────────────

#[derive(Template)]
#[template(path = "report.html")]
struct AnalysisHtmlReport<'a> {
    title: &'a str,
    generated_at: String,
    summary: &'a Summary,
    by_language: Vec<(&'a str, &'a LanguageSummary)>,
    elapsed_secs: f64,
}

// ── Health ───────────────────────────────────────────────

struct HtmlDimensionScore {
    dimension: String,
    score_display: u32,
    grade: Grade,
}

struct HtmlDirectoryHealth {
    path: String,
    score_display: u32,
    grade: Grade,
    file_count: usize,
}

struct HtmlFileHealth {
    path: String,
    score_display: u32,
    grade: Grade,
    top_issue: String,
}

struct HtmlLanguageHealth {
    language: String,
    score_display: u32,
    grade: Grade,
    file_count: usize,
    code_lines: usize,
    confidence: String,
    tail_score: u32,
}

struct HtmlLanguageHealthChange {
    language: String,
    status: String,
    before: String,
    after: String,
    delta: String,
}

#[derive(Template)]
#[template(path = "health.html")]
struct HealthHtmlReport {
    generated_at: String,
    model: String,
    grade: String,
    score: u32,
    scope: String,
    confidence: String,
    tail_score: u32,
    failing_files: usize,
    failing_ratio: u32,
    test_health: String,
    baseline_comparison: String,
    scope_change: String,
    comparison_verdict: String,
    language_changes: Vec<HtmlLanguageHealthChange>,
    dimensions: Vec<HtmlDimensionScore>,
    by_language: Vec<HtmlLanguageHealth>,
    by_directory: Vec<HtmlDirectoryHealth>,
    worst_files: Vec<HtmlFileHealth>,
}

fn html_language_health_change(
    language: &crate::insight::health::LanguageHealthChange,
) -> HtmlLanguageHealthChange {
    HtmlLanguageHealthChange {
        language: language.language.clone(),
        status: language.status.to_string(),
        before: format_optional_health(language.from_grade, language.from_score),
        after: format_optional_health(language.to_grade, language.to_score),
        delta: format_optional_delta(language.score_delta),
    }
}

fn format_optional_health(grade: Option<Grade>, score: Option<f64>) -> String {
    match (grade, score) {
        (Some(grade), Some(score)) => format!("{grade} ({score:.1})"),
        _ => "-".to_string(),
    }
}

fn format_optional_delta(delta: Option<f64>) -> String {
    match delta {
        Some(delta) if delta >= 0.0 => format!("+{delta:.1}"),
        Some(delta) => format!("{delta:.1}"),
        None => "-".to_string(),
    }
}

fn health_model_display_name(model: &str) -> String {
    match model {
        "default" => "v1".to_string(),
        "default-no-dup" => "v1 (no duplication scan)".to_string(),
        "default-v2" => "v2".to_string(),
        "default-v2-no-dup" => "v2 (no duplication scan)".to_string(),
        other => other.to_string(),
    }
}

fn html_language_health(language: &crate::insight::health::LanguageHealth) -> HtmlLanguageHealth {
    HtmlLanguageHealth {
        language: language.language.clone(),
        score_display: language.score as u32,
        grade: language.grade,
        file_count: language.file_count,
        code_lines: language.code_lines,
        confidence: format!(
            "{} ({:.0}%)",
            language.confidence.level,
            language.confidence.coverage * 100.0
        ),
        tail_score: language.tail_risk.tail_score as u32,
    }
}

// ── Hotspot ──────────────────────────────────────────────

struct HtmlFileHotspot {
    path: String,
    language: String,
    commits: usize,
    lines_added: usize,
    lines_deleted: usize,
    cyclomatic: usize,
    age: String,
    authors: String,
    score_display: String,
    score_pct: u32,
    risk: String,
}

#[derive(Template)]
#[template(path = "hotspot.html")]
struct HotspotHtmlReport {
    generated_at: String,
    since: String,
    total_commits: usize,
    files: Vec<HtmlFileHotspot>,
}

// ── Coupling ─────────────────────────────────────────────

struct HtmlCouplingPair {
    file_a: String,
    file_b: String,
    shared_commits: usize,
    commits_a: usize,
    commits_b: usize,
    degree_display: String,
    degree_pct: u32,
}

#[derive(Template)]
#[template(path = "coupling.html")]
struct CouplingHtmlReport {
    generated_at: String,
    since: String,
    total_commits: usize,
    pairs: Vec<HtmlCouplingPair>,
}

// ── Trend ────────────────────────────────────────────────

struct HtmlTrendMetric {
    label: String,
    from_value: usize,
    to_value: usize,
    signed_delta: i64,
    delta_display: String,
    percent_display: String,
}

struct HtmlLanguageTrend {
    language: String,
    status: String,
    code_from: usize,
    code_to: usize,
    code_delta: i64,
    code_delta_display: String,
}

struct HtmlHistoryPoint {
    date: String,
    label: String,
    code: usize,
    cyclomatic: usize,
    files: usize,
}

#[derive(Template)]
#[template(path = "trend.html")]
struct TrendHtmlReport {
    from_date: String,
    from_label: String,
    to_date: String,
    to_label: String,
    health_model: String,
    metrics: Vec<HtmlTrendMetric>,
    by_language: Vec<HtmlLanguageTrend>,
    health_scope_change: String,
    health_by_language: Vec<HtmlLanguageHealthChange>,
    history: Vec<HtmlHistoryPoint>,
}

// ── Estimation ──────────────────────────────────────────

struct HtmlLanguageEstimation {
    language: String,
    code_lines: usize,
    effort_months: String,
    cost: String,
    /// Raw f64 values for chart.js data attributes.
    cost_raw: f64,
    effort_raw: f64,
}

struct HtmlEstimationParam {
    key: String,
    value: String,
}

#[derive(Template)]
#[template(path = "estimation.html")]
struct EstimationHtmlReport {
    generated_at: String,
    model: String,
    total_sloc: usize,
    estimated_cost: String,
    schedule_months: String,
    people_required: String,
    by_language: Vec<HtmlLanguageEstimation>,
    params: Vec<HtmlEstimationParam>,
}

// ── Estimation Comparison ────────────────────────────────

struct HtmlComparisonRow {
    model: String,
    effort_months: String,
    schedule_months: String,
    people_required: String,
    estimated_cost: String,
    cost_raw: f64,
}

#[derive(Template)]
#[template(path = "estimation_comparison.html")]
struct EstimationComparisonHtmlReport {
    generated_at: String,
    total_sloc: usize,
    rows: Vec<HtmlComparisonRow>,
}

// ── Combined (default command: stats + health + estimation) ──

#[derive(Template)]
#[template(path = "combined.html")]
struct CombinedHtmlReport<'a> {
    title: &'a str,
    generated_at: String,
    summary: &'a Summary,
    by_language: Vec<(&'a str, &'a LanguageSummary)>,
    elapsed_secs: f64,
    health_grade: String,
    health_score: u32,
    health_model: String,
    health_scope: String,
    health_confidence: String,
    health_tail_score: u32,
    health_failing_files: usize,
    health_test: String,
    dimensions: Vec<HtmlDimensionScore>,
    health_languages: Vec<HtmlLanguageHealth>,
    worst_files: Vec<HtmlFileHealth>,
    total_sloc: usize,
    estimation_rows: Vec<HtmlComparisonRow>,
}

// ── OutputFormat impl ────────────────────────────────────

pub struct HtmlOutput;

impl HtmlOutput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HtmlOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for HtmlOutput {
    fn name(&self) -> &'static str {
        "html"
    }

    fn extension(&self) -> &'static str {
        "html"
    }

    fn write(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(report) => self.write_health(report, writer),
            Report::Hotspot(report) => self.write_hotspot(report, writer),
            Report::Coupling(report) => self.write_coupling(report, writer),
            Report::Diff(report) => self.write_diff(report, writer),
            Report::Trend(report) => self.write_trend(report, writer),
            Report::Estimation(report) => self.write_estimation(report, writer),
            Report::EstimationComparison(report) => {
                self.write_estimation_comparison(report, writer)
            }
            Report::Combined(combined) => self.write_combined(combined, options, writer),
        }
    }
}

impl HtmlOutput {
    fn write_analysis(
        &self,
        result: &AnalysisResult,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut by_language: Vec<_> = result
            .summary
            .by_language
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        if let Some(n) = options.top_n {
            by_language.truncate(n);
        }

        let report = AnalysisHtmlReport {
            title: "Codelens - Code Statistics Report",
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            summary: &result.summary,
            by_language,
            elapsed_secs: result.elapsed.as_secs_f64(),
        };

        write!(writer, "{}", report.render()?)?;
        Ok(())
    }

    fn write_combined(
        &self,
        combined: &crate::output::format::CombinedReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let result = &combined.analysis;
        let health = &combined.health;
        let estimation = &combined.estimation;

        let mut by_language: Vec<_> = result
            .summary
            .by_language
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();
        if let Some(n) = options.top_n {
            by_language.truncate(n);
        }

        let html = CombinedHtmlReport {
            title: "Codelens - Code Analysis Report",
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            summary: &result.summary,
            by_language,
            elapsed_secs: result.elapsed.as_secs_f64(),
            health_grade: health.grade.to_string(),
            health_score: health.score as u32,
            health_model: health_model_display_name(&health.model),
            health_scope: health.scope.to_string(),
            health_confidence: format!(
                "{} ({:.0}%)",
                health.confidence.level,
                health.confidence.coverage * 100.0
            ),
            health_tail_score: health.tail_risk.tail_score as u32,
            health_failing_files: health.tail_risk.failing_files,
            health_test: health
                .test_health
                .as_ref()
                .map(|test| {
                    format!(
                        "{} ({:.1}, {} files)",
                        test.grade, test.score, test.file_count
                    )
                })
                .unwrap_or_default(),
            dimensions: health
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score as u32,
                    grade: d.grade,
                })
                .collect(),
            health_languages: health
                .by_language
                .iter()
                .map(html_language_health)
                .collect(),
            worst_files: health
                .worst_files
                .iter()
                .map(|f| HtmlFileHealth {
                    path: f.path.display().to_string(),
                    score_display: f.score as u32,
                    grade: f.grade,
                    top_issue: f.top_issue.to_string(),
                })
                .collect(),
            total_sloc: estimation.total_sloc,
            estimation_rows: estimation
                .reports
                .iter()
                .map(|r| HtmlComparisonRow {
                    model: r.model.clone(),
                    effort_months: format!("{:.2}", r.effort_months),
                    schedule_months: format!("{:.2}", r.schedule_months),
                    people_required: format!("{:.2}", r.people_required),
                    estimated_cost: format!("{:.2}", r.estimated_cost),
                    cost_raw: r.estimated_cost,
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_health(
        &self,
        report: &crate::insight::health::HealthReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let regression = report.regression.as_ref();
        let html = HealthHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            model: health_model_display_name(&report.model),
            grade: report.grade.to_string(),
            score: report.score as u32,
            scope: report.scope.to_string(),
            confidence: format!(
                "{} ({:.0}%)",
                report.confidence.level,
                report.confidence.coverage * 100.0
            ),
            tail_score: report.tail_risk.tail_score as u32,
            failing_files: report.tail_risk.failing_files,
            failing_ratio: (report.tail_risk.failing_ratio * 100.0).round() as u32,
            test_health: report
                .test_health
                .as_ref()
                .map(|test| {
                    format!(
                        "{} ({:.1}, {} files)",
                        test.grade, test.score, test.file_count
                    )
                })
                .unwrap_or_default(),
            baseline_comparison: regression
                .map(|comparison| {
                    format!(
                        "{} using {}: {} ({:.1}) → {} ({:.1}), Δ {}",
                        comparison.baseline,
                        health_model_display_name(&comparison.model),
                        comparison.baseline_grade,
                        comparison.baseline_score,
                        comparison.current_grade,
                        comparison.current_score,
                        format_optional_delta(Some(comparison.score_delta)),
                    )
                })
                .unwrap_or_default(),
            scope_change: regression
                .filter(|comparison| comparison.scope_changed)
                .map(|comparison| {
                    format!(
                        "Scope changed: {} → {}. Project delta is informational.",
                        comparison.baseline_scope, comparison.current_scope
                    )
                })
                .unwrap_or_default(),
            comparison_verdict: regression
                .map(|comparison| {
                    if comparison.failed {
                        "REGRESSED".to_string()
                    } else {
                        "NO REGRESSION".to_string()
                    }
                })
                .unwrap_or_default(),
            language_changes: regression
                .map(|comparison| {
                    comparison
                        .by_language
                        .iter()
                        .map(html_language_health_change)
                        .collect()
                })
                .unwrap_or_default(),
            dimensions: report
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score as u32,
                    grade: d.grade,
                })
                .collect(),
            by_language: report
                .by_language
                .iter()
                .map(html_language_health)
                .collect(),
            by_directory: report
                .by_directory
                .iter()
                .map(|d| HtmlDirectoryHealth {
                    path: d.path.display().to_string(),
                    score_display: d.score as u32,
                    grade: d.grade,
                    file_count: d.file_count,
                })
                .collect(),
            worst_files: report
                .worst_files
                .iter()
                .map(|f| HtmlFileHealth {
                    path: f.path.display().to_string(),
                    score_display: f.score as u32,
                    grade: f.grade,
                    top_issue: f.top_issue.to_string(),
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_hotspot(
        &self,
        report: &crate::insight::hotspot::HotspotReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let html = HotspotHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            since: report.since.clone(),
            total_commits: report.total_commits,
            files: report
                .files
                .iter()
                .map(|h| HtmlFileHotspot {
                    path: h.path.display().to_string(),
                    language: h.language.clone(),
                    commits: h.churn.commits,
                    lines_added: h.churn.lines_added,
                    lines_deleted: h.churn.lines_deleted,
                    cyclomatic: h.complexity.cyclomatic,
                    age: h
                        .age_days
                        .map(crate::insight::hotspot::format_age)
                        .unwrap_or_else(|| "-".to_string()),
                    authors: match &h.knowledge {
                        Some(k) if k.knowledge_island => format!(
                            "{} ★ ({} {:.0}%)",
                            k.authors,
                            k.main_author,
                            k.ownership * 100.0
                        ),
                        Some(k) => k.authors.to_string(),
                        None => "-".to_string(),
                    },
                    score_display: format!("{:.2}", h.hotspot_score),
                    score_pct: (h.hotspot_score * 100.0) as u32,
                    risk: h.risk.to_string(),
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_coupling(
        &self,
        report: &crate::insight::coupling::CouplingReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let html = CouplingHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            since: report.since.clone(),
            total_commits: report.total_commits,
            pairs: report
                .pairs
                .iter()
                .map(|p| HtmlCouplingPair {
                    file_a: p.file_a.display().to_string(),
                    file_b: p.file_b.display().to_string(),
                    shared_commits: p.shared_commits,
                    commits_a: p.commits_a,
                    commits_b: p.commits_b,
                    degree_display: format!("{:.0}%", p.degree),
                    degree_pct: (p.degree.min(100.0)) as u32,
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_diff(
        &self,
        report: &crate::insight::diff::DiffReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        // The trend template renders before/after comparisons; a git-ref
        // diff is exactly that with ref names in place of snapshot dates.
        let make_metric = |label: &str, dv: &crate::insight::DeltaValue<usize>| {
            let signed = dv.signed_delta();
            HtmlTrendMetric {
                label: label.to_string(),
                from_value: dv.from,
                to_value: dv.to,
                signed_delta: signed,
                delta_display: if signed >= 0 {
                    format!("+{signed}")
                } else {
                    format!("{signed}")
                },
                percent_display: format!("{:+.1}%", dv.percent),
            }
        };
        let d = &report.delta;
        let metrics = vec![
            make_metric("Files", &d.files),
            make_metric("Code", &d.code),
            make_metric("Comments", &d.comment),
            make_metric("Complexity", &d.complexity),
            make_metric("Functions", &d.functions),
        ];
        let by_language = report
            .by_language
            .iter()
            .map(|lt| {
                let signed = lt.code.signed_delta();
                HtmlLanguageTrend {
                    language: lt.language.clone(),
                    status: lt.status.to_string(),
                    code_from: lt.code.from,
                    code_to: lt.code.to,
                    code_delta: signed,
                    code_delta_display: if signed >= 0 {
                        format!("+{signed}")
                    } else {
                        format!("{signed}")
                    },
                }
            })
            .collect();

        let html = TrendHtmlReport {
            from_date: report.from.clone(),
            from_label: format!(
                "health {} ({:.1})",
                report.health.baseline_grade, report.health.baseline_score
            ),
            to_date: report.to.clone(),
            to_label: format!("health {} ({:.1})", report.to_grade, report.to_score),
            health_model: health_model_display_name(&report.health.model),
            metrics,
            by_language,
            health_scope_change: if report.health.scope_changed {
                format!(
                    "Scope changed: {} → {}. Project delta is informational.",
                    report.health.baseline_scope, report.health.current_scope
                )
            } else {
                String::new()
            },
            health_by_language: report
                .health
                .by_language
                .iter()
                .map(html_language_health_change)
                .collect(),
            history: Vec::new(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_trend(
        &self,
        report: &crate::insight::trend::TrendReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let d = &report.delta;
        let make_metric =
            |label: &str, dv: &crate::insight::DeltaValue<usize>| -> HtmlTrendMetric {
                let signed = dv.signed_delta();
                HtmlTrendMetric {
                    label: label.to_string(),
                    from_value: dv.from,
                    to_value: dv.to,
                    signed_delta: signed,
                    delta_display: if signed >= 0 {
                        format!("+{signed}")
                    } else {
                        format!("{signed}")
                    },
                    percent_display: format!("{:+.1}%", dv.percent),
                }
            };

        let metrics = vec![
            make_metric("Files", &d.files),
            make_metric("Code", &d.code),
            make_metric("Comments", &d.comment),
            make_metric("Blank", &d.blank),
            make_metric("Complexity", &d.complexity),
            make_metric("Functions", &d.functions),
        ];

        let by_language: Vec<HtmlLanguageTrend> = report
            .by_language
            .iter()
            .map(|lt| {
                let signed = lt.code.signed_delta();
                HtmlLanguageTrend {
                    language: lt.language.clone(),
                    status: lt.status.to_string(),
                    code_from: lt.code.from,
                    code_to: lt.code.to,
                    code_delta: signed,
                    code_delta_display: if signed >= 0 {
                        format!("+{signed}")
                    } else {
                        format!("{signed}")
                    },
                }
            })
            .collect();

        // The history chart needs at least two points to be a line.
        let history: Vec<HtmlHistoryPoint> = if report.history.len() >= 2 {
            report
                .history
                .iter()
                .map(|p| HtmlHistoryPoint {
                    date: p.timestamp.format("%Y-%m-%d").to_string(),
                    label: p.label.clone().unwrap_or_default(),
                    code: p.code,
                    cyclomatic: p.cyclomatic,
                    files: p.files,
                })
                .collect()
        } else {
            Vec::new()
        };

        let html = TrendHtmlReport {
            from_date: report.from.timestamp.format("%Y-%m-%d").to_string(),
            from_label: report.from.label.as_deref().unwrap_or("").to_string(),
            to_date: report.to.timestamp.format("%Y-%m-%d").to_string(),
            to_label: report.to.label.as_deref().unwrap_or("").to_string(),
            health_model: String::new(),
            metrics,
            by_language,
            health_scope_change: String::new(),
            health_by_language: Vec::new(),
            history,
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_estimation(
        &self,
        report: &crate::insight::estimation::EstimationReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let html = EstimationHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            model: report.model.clone(),
            total_sloc: report.total_sloc,
            estimated_cost: format!("{:.2}", report.estimated_cost),
            schedule_months: format!("{:.2}", report.schedule_months),
            people_required: format!("{:.2}", report.people_required),
            by_language: report
                .by_language
                .iter()
                .map(|l| HtmlLanguageEstimation {
                    language: l.language.clone(),
                    code_lines: l.code_lines,
                    effort_months: format!("{:.2}", l.effort_months),
                    cost: format!("{:.2}", l.cost),
                    cost_raw: l.cost,
                    effort_raw: l.effort_months,
                })
                .collect(),
            params: report
                .params
                .iter()
                .map(|(k, v)| HtmlEstimationParam {
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let html = EstimationComparisonHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            total_sloc: report.total_sloc,
            rows: report
                .reports
                .iter()
                .map(|r| HtmlComparisonRow {
                    model: r.model.clone(),
                    effort_months: format!("{:.2}", r.effort_months),
                    schedule_months: format!("{:.2}", r.schedule_months),
                    people_required: format!("{:.2}", r.people_required),
                    estimated_cost: format!("{:.2}", r.estimated_cost),
                    cost_raw: r.estimated_cost,
                })
                .collect(),
        };
        write!(writer, "{}", html.render()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{AnalysisResult, FileStats, LineStats, Summary};
    use crate::insight::health::{
        Confidence, HealthReport, HealthScope, LanguageHealth, LanguageHealthChange,
        LanguageHealthChangeStatus, RegressionReport, TailRisk,
    };
    use crate::insight::scoring::default::DefaultModel;
    use crate::output::{OutputFormat, OutputOptions, Report};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn health_model_display_name_is_user_facing() {
        assert_eq!(health_model_display_name("default"), "v1");
        assert_eq!(
            health_model_display_name("default-no-dup"),
            "v1 (no duplication scan)"
        );
        assert_eq!(health_model_display_name("default-v2"), "v2");
        assert_eq!(
            health_model_display_name("default-v2-no-dup"),
            "v2 (no duplication scan)"
        );
        assert_eq!(health_model_display_name("custom"), "custom");
    }

    #[test]
    fn health_html_renders_language_health_before_file_details() {
        let report = HealthReport {
            score: 82.0,
            grade: Grade::B,
            model: "default-v2".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![],
            scope: HealthScope::Production,
            confidence: Confidence::from_coverage(1.0),
            tail_risk: TailRisk::default(),
            by_language: vec![LanguageHealth {
                language: "Rust".to_string(),
                score: 82.0,
                grade: Grade::B,
                file_count: 4,
                code_lines: 300,
                confidence: Confidence::from_coverage(1.0),
                dimensions: vec![],
                tail_risk: TailRisk::default(),
            }],
            test_health: None,
            regression: Some(RegressionReport {
                baseline: "main".to_string(),
                model: "default-v2".to_string(),
                baseline_score: 84.0,
                baseline_grade: Grade::B,
                baseline_scope: HealthScope::TestsOnly,
                current_score: 82.0,
                current_grade: Grade::B,
                current_scope: HealthScope::Production,
                scope_changed: true,
                score_delta: -2.0,
                project_regressed: false,
                regressed_files: vec![],
                improved_files: 0,
                by_language: vec![LanguageHealthChange {
                    language: "Rust".to_string(),
                    status: LanguageHealthChangeStatus::Changed,
                    from_score: Some(84.0),
                    from_grade: Some(Grade::B),
                    to_score: Some(82.0),
                    to_grade: Some(Grade::B),
                    score_delta: Some(-2.0),
                }],
                failed: false,
            }),
        };
        let mut output = Vec::new();

        HtmlOutput::new()
            .write(
                &Report::Health(report),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();

        let html = String::from_utf8(output).unwrap();
        let language_summary = html.find("By Language").unwrap();
        let language_change = html.find("Language Health Changes").unwrap();
        let file_details = html.find("Worst Files").unwrap_or(html.len());
        assert!(language_summary < language_change);
        assert!(language_change < file_details);
        assert!(html.contains("Scope changed: tests-only → production"));
        assert!(html.contains("B (84.0)"));
        assert!(html.contains("B (82.0)"));
        assert!(html.contains("Model: v2"));
        assert!(!html.contains("Model: default-v2"));
    }

    #[test]
    fn diff_html_identifies_the_health_model() {
        let files = vec![FileStats {
            path: PathBuf::from("src/lib.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 12,
                code: 10,
                comment: 1,
                blank: 1,
            },
            ..FileStats::default()
        }];
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(1),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };
        let report =
            crate::insight::diff::build("before", "after", &result, &result, &DefaultModel::new());
        let mut output = Vec::new();

        HtmlOutput::new()
            .write(
                &Report::Diff(Box::new(report)),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();

        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("Health model: v2"));
        assert!(!html.contains("Health model: default-v2"));
    }
}
