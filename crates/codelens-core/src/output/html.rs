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

#[derive(Template)]
#[template(path = "health.html")]
struct HealthHtmlReport {
    generated_at: String,
    model: String,
    grade: String,
    score: u32,
    dimensions: Vec<HtmlDimensionScore>,
    by_directory: Vec<HtmlDirectoryHealth>,
    worst_files: Vec<HtmlFileHealth>,
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
    metrics: Vec<HtmlTrendMetric>,
    by_language: Vec<HtmlLanguageTrend>,
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
    dimensions: Vec<HtmlDimensionScore>,
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
            health_model: health.model.clone(),
            dimensions: health
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score as u32,
                    grade: d.grade,
                })
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
        let html = HealthHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            model: report.model.clone(),
            grade: report.grade.to_string(),
            score: report.score as u32,
            dimensions: report
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score as u32,
                    grade: d.grade,
                })
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
            metrics,
            by_language,
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
            metrics,
            by_language,
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
