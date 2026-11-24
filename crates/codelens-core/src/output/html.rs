//! HTML output format with interactive charts.

use std::io::Write;

use askama::Template;

use crate::analyzer::stats::{AnalysisResult, LanguageSummary, Summary};
use crate::error::Result;
use crate::insight::health::HealthAssessment;
use crate::insight::scoring::HealthDimension;
use crate::insight::Grade;

use super::format::{
    health_model_display_name, AnalysisPresentation, HealthDirectoryPresentation,
    HealthFilePresentation, HealthLanguagePresentation, HealthPresentation, OutputFormat,
    OutputOptions, Report,
};

// ── Analysis (existing) ──────────────────────────────────

#[derive(Template)]
#[template(path = "report.html")]
struct AnalysisHtmlReport {
    title: &'static str,
    generated_at: String,
    view: AnalysisPresentation,
}

// ── Health ───────────────────────────────────────────────

struct HtmlDimensionScore {
    dimension: String,
    score_display: String,
    grade: Grade,
    explanation: String,
}

#[derive(Clone)]
struct HtmlDirectoryHealth {
    path: String,
    score_display: String,
    grade: Grade,
    file_count: usize,
}

#[derive(Clone)]
struct HtmlFileHealth {
    path: String,
    score_display: String,
    grade: Grade,
    top_issue: String,
    priority_reason: String,
    suggested_check: String,
    focus_command: String,
}

struct HtmlLanguageHealth {
    language: String,
    score_display: String,
    grade: String,
    file_count: usize,
    code_lines: usize,
    confidence: String,
    tail_score: String,
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
    tool_version: &'static str,
    model: String,
    grade: String,
    score: String,
    is_scored: bool,
    scope: String,
    confidence: String,
    tail_score: String,
    failing_files: usize,
    failing_ratio: String,
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

fn html_language_health(language: &HealthLanguagePresentation) -> HtmlLanguageHealth {
    HtmlLanguageHealth {
        language: language.language.clone(),
        score_display: if language.estimated {
            format!("Estimated {}", language.score_display)
        } else {
            language.score_display.clone()
        },
        grade: language.grade_display.clone(),
        file_count: language.file_count,
        code_lines: language.code_lines,
        confidence: language.confidence.clone(),
        tail_score: language.tail_score.clone(),
    }
}

fn html_directory_health(directory: &HealthDirectoryPresentation) -> HtmlDirectoryHealth {
    HtmlDirectoryHealth {
        path: directory.path.clone(),
        score_display: directory.score_display.clone(),
        grade: directory.grade,
        file_count: directory.file_count,
    }
}

fn html_file_health(file: &HealthFilePresentation) -> HtmlFileHealth {
    let path = file.path.clone();
    let score_display = file.score_display.clone();
    let parent = std::path::Path::new(&file.path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    HtmlFileHealth {
        priority_reason: format!(
            "Grade {} ({}). {} is the largest health drag in this file.",
            file.grade, score_display, file.top_issue
        ),
        suggested_check: health_dimension_suggested_check(file.top_issue).to_string(),
        focus_command: format!(
            "codelens health {} --top 5",
            shell_quote_for_display(&parent)
        ),
        path,
        score_display,
        grade: file.grade,
        top_issue: file.top_issue.to_string(),
    }
}

fn health_dimension_explanation(dimension: HealthDimension) -> &'static str {
    match dimension {
        HealthDimension::Complexity => {
            "Independent paths through the measured code; more paths are harder to reason about."
        }
        HealthDimension::FuncSize => {
            "Whether measured functions stay small enough to understand and review."
        }
        HealthDimension::CommentRatio => {
            "Whether the analyzed code contains enough explanation; the right amount depends on context."
        }
        HealthDimension::FileSize => {
            "Whether files stay focused enough to navigate, review, and change safely."
        }
        HealthDimension::NestingDepth => {
            "How deeply conditions and loops are nested; deeper nesting is harder to follow."
        }
        HealthDimension::Duplication => {
            "How much line-level content repeats across the analyzed code."
        }
    }
}

fn health_dimension_suggested_check(dimension: HealthDimension) -> &'static str {
    match dimension {
        HealthDimension::Complexity => {
            "Inspect the most complex functions and confirm which branches are essential before refactoring."
        }
        HealthDimension::FuncSize => {
            "Check whether the largest functions mix responsibilities that can be reviewed separately."
        }
        HealthDimension::CommentRatio => {
            "Review non-obvious behavior and public contracts; add explanation only where it improves understanding."
        }
        HealthDimension::FileSize => {
            "Check whether this file combines unrelated responsibilities before deciding to split it."
        }
        HealthDimension::NestingDepth => {
            "Locate deeply nested functions and consider guard clauses or extracting independent branches."
        }
        HealthDimension::Duplication => {
            "Confirm repeated lines represent the same behavior before consolidating them."
        }
    }
}

fn shell_quote_for_display(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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

struct HtmlFunctionHotspot {
    name: String,
    start_line: usize,
    end_line: usize,
    commits: usize,
    cyclomatic: usize,
}

struct HtmlFunctionHotspotFile {
    path: String,
    functions: Vec<HtmlFunctionHotspot>,
}

#[derive(Template)]
#[template(path = "hotspot.html")]
struct HotspotHtmlReport {
    generated_at: String,
    since: String,
    total_commits: usize,
    files: Vec<HtmlFileHotspot>,
    function_files: Vec<HtmlFunctionHotspotFile>,
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
    focus: Option<String>,
    skipped_large_commits: usize,
    excluded_test_files: usize,
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

struct HtmlCombinedEstimationRow {
    model: String,
    effort_months: String,
    schedule_months: String,
    people_required: String,
    estimated_cost: String,
    params: Vec<HtmlEstimationParam>,
}

#[derive(Template)]
#[template(path = "estimation_comparison.html")]
struct EstimationComparisonHtmlReport {
    generated_at: String,
    total_sloc: usize,
    rows: Vec<HtmlComparisonRow>,
}

// ── Combined (default command: stats + health + estimation) ──

struct HtmlHealthDecision {
    label: String,
    tone: &'static str,
    summary: String,
}

fn combined_health_decision(
    report: &crate::insight::health::HealthReport,
    view: &HealthPresentation,
) -> HtmlHealthDecision {
    let weakest_dimension = report
        .dimensions
        .iter()
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .map(|dimension| {
            format!(
                " The weakest measured dimension is {} ({} {:.1}).",
                dimension.dimension, dimension.grade, dimension.score
            )
        })
        .unwrap_or_default();

    if report.assessment == HealthAssessment::InsufficientData {
        return HtmlHealthDecision {
            label: "Insufficient data".to_string(),
            tone: "neutral",
            summary: format!(
                "The scan does not have enough measured data for a reliable letter grade. Treat the {} score as an estimate and review measurement coverage before acting.{}",
                view.score, weakest_dimension
            ),
        };
    }

    if view.failing_files > 0 {
        let starting_point = report
            .worst_files
            .first()
            .map(|file| format!(" Start with {}.", file.path.display()))
            .unwrap_or_default();
        return HtmlHealthDecision {
            label: "Needs attention".to_string(),
            tone: "warning",
            summary: format!(
                "Overall health is {} ({}), but {} evaluated file(s) are graded F.{}{}",
                report.grade, view.score, view.failing_files, starting_point, weakest_dimension
            ),
        };
    }

    let (label, tone) = if report.grade <= Grade::B {
        ("No F-grade files detected", "good")
    } else {
        ("Needs attention", "warning")
    };
    HtmlHealthDecision {
        label: label.to_string(),
        tone,
        summary: format!(
            "Overall health is {} ({}), with no F-grade files among the evaluated files.{}",
            report.grade, view.score, weakest_dimension
        ),
    }
}

#[derive(Template)]
#[template(path = "combined.html")]
struct CombinedHtmlReport<'a> {
    title: &'a str,
    generated_at: String,
    tool_version: &'static str,
    summary: &'a Summary,
    by_language: Vec<(&'a str, &'a LanguageSummary)>,
    elapsed_secs: f64,
    health_grade: String,
    health_score: String,
    health_is_scored: bool,
    health_model: String,
    health_scope: String,
    health_confidence: String,
    health_tail_score: String,
    health_failing_files: usize,
    health_failing_ratio: String,
    health_test: String,
    health_decision_label: String,
    health_decision_tone: &'static str,
    health_decision_summary: String,
    dimensions: Vec<HtmlDimensionScore>,
    health_languages: Vec<HtmlLanguageHealth>,
    worst_directories: Vec<HtmlDirectoryHealth>,
    worst_files: Vec<HtmlFileHealth>,
    priority_files: Vec<HtmlFileHealth>,
    show_history_sections: bool,
    git_history_note: String,
    hotspot_since: String,
    hotspot_total_commits: usize,
    hotspots: Vec<HtmlFileHotspot>,
    coupling_since: String,
    coupling_total_commits: usize,
    coupling_excluded_test_files: usize,
    coupling_pairs: Vec<HtmlCouplingPair>,
    trend_note: String,
    trend_available: bool,
    trend_from_date: String,
    trend_from_label: String,
    trend_to_date: String,
    trend_metrics: Vec<HtmlTrendMetric>,
    trend_languages: Vec<HtmlLanguageTrend>,
    trend_history: Vec<HtmlHistoryPoint>,
    total_sloc: usize,
    cost_scope: String,
    traditional_estimation_rows: Vec<HtmlCombinedEstimationRow>,
    locomo_estimation_rows: Vec<HtmlCombinedEstimationRow>,
}

// ── OutputFormat impl ────────────────────────────────────

pub struct HtmlOutput;

impl HtmlOutput {
    pub fn new() -> Self {
        Self
    }

    /// Render the default HTML report with optional Git and snapshot history.
    ///
    /// This entry point is used by the CLI's default HTML command. Other
    /// output formats keep the stable `CombinedReport` schema unchanged.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn write_combined_dashboard(
        &self,
        combined: &crate::output::format::CombinedReport,
        hotspot: Option<&crate::insight::hotspot::HotspotReport>,
        coupling: Option<&crate::insight::coupling::CouplingReport>,
        trend: Option<&crate::insight::trend::TrendReport>,
        git_history_note: &str,
        trend_note: &str,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        self.write_combined_report(
            combined,
            hotspot,
            coupling,
            trend,
            git_history_note,
            trend_note,
            true,
            options,
            writer,
        )
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
        let report = AnalysisHtmlReport {
            title: "Codelens - Code Statistics Report",
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            view: AnalysisPresentation::from_result(result, options),
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
        self.write_combined_report(combined, None, None, None, "", "", false, options, writer)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_combined_report(
        &self,
        combined: &crate::output::format::CombinedReport,
        hotspot: Option<&crate::insight::hotspot::HotspotReport>,
        coupling: Option<&crate::insight::coupling::CouplingReport>,
        trend: Option<&crate::insight::trend::TrendReport>,
        git_history_note: &str,
        trend_note: &str,
        show_history_sections: bool,
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

        let health_view = HealthPresentation::from_report(health);
        let health_decision = combined_health_decision(health, &health_view);
        let worst_files: Vec<_> = health_view
            .worst_files
            .iter()
            .map(html_file_health)
            .collect();
        let priority_files = worst_files.iter().take(3).cloned().collect();
        let mut traditional_estimation_rows = Vec::new();
        let mut locomo_estimation_rows = Vec::new();
        for report in &estimation.reports {
            let row = HtmlCombinedEstimationRow {
                model: report.model.clone(),
                effort_months: format!("{:.2}", report.effort_months),
                schedule_months: format!("{:.2}", report.schedule_months),
                people_required: format!("{:.2}", report.people_required),
                estimated_cost: format!("{:.2}", report.estimated_cost),
                params: report
                    .params
                    .iter()
                    .map(|(key, value)| HtmlEstimationParam {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            };
            if report.model == "LOCOMO" {
                locomo_estimation_rows.push(row);
            } else {
                traditional_estimation_rows.push(row);
            }
        }
        let counted_languages = result
            .summary
            .by_language
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        let hotspots = hotspot
            .map(|report| {
                report
                    .files
                    .iter()
                    .map(|file| HtmlFileHotspot {
                        path: file.path.display().to_string(),
                        language: file.language.clone(),
                        commits: file.churn.commits,
                        lines_added: file.churn.lines_added,
                        lines_deleted: file.churn.lines_deleted,
                        cyclomatic: file.complexity.cyclomatic,
                        age: file
                            .age_days
                            .map(crate::insight::hotspot::format_age)
                            .unwrap_or_else(|| "-".to_string()),
                        authors: match &file.knowledge {
                            Some(knowledge) if knowledge.knowledge_island => format!(
                                "{} · {} {:.0}% · knowledge island",
                                knowledge.authors,
                                knowledge.main_author,
                                knowledge.ownership * 100.0
                            ),
                            Some(knowledge) => knowledge.authors.to_string(),
                            None => "-".to_string(),
                        },
                        score_display: format!("{:.2}", file.hotspot_score),
                        score_pct: (file.hotspot_score * 100.0) as u32,
                        risk: file.risk.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let coupling_pairs = coupling
            .map(|report| {
                report
                    .pairs
                    .iter()
                    .map(|pair| HtmlCouplingPair {
                        file_a: pair.file_a.display().to_string(),
                        file_b: pair.file_b.display().to_string(),
                        shared_commits: pair.shared_commits,
                        commits_a: pair.commits_a,
                        commits_b: pair.commits_b,
                        degree_display: format!("{:.0}%", pair.degree),
                        degree_pct: pair.degree.clamp(0.0, 100.0) as u32,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let make_trend_metric =
            |label: &str, delta: &crate::insight::DeltaValue<usize>| -> HtmlTrendMetric {
                let signed_delta = delta.signed_delta();
                HtmlTrendMetric {
                    label: label.to_string(),
                    from_value: delta.from,
                    to_value: delta.to,
                    signed_delta,
                    delta_display: if signed_delta >= 0 {
                        format!("+{signed_delta}")
                    } else {
                        signed_delta.to_string()
                    },
                    percent_display: format!("{:+.1}%", delta.percent),
                }
            };
        let trend_metrics = trend
            .map(|report| {
                vec![
                    make_trend_metric("Files", &report.delta.files),
                    make_trend_metric("Code lines", &report.delta.code),
                    make_trend_metric("Complexity", &report.delta.complexity),
                    make_trend_metric("Functions", &report.delta.functions),
                ]
            })
            .unwrap_or_default();
        let mut trend_languages = trend
            .map(|report| {
                report
                    .by_language
                    .iter()
                    .map(|language| {
                        let signed_delta = language.code.signed_delta();
                        HtmlLanguageTrend {
                            language: language.language.clone(),
                            status: language.status.to_string(),
                            code_from: language.code.from,
                            code_to: language.code.to,
                            code_delta: signed_delta,
                            code_delta_display: if signed_delta >= 0 {
                                format!("+{signed_delta}")
                            } else {
                                signed_delta.to_string()
                            },
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(top_n) = options.top_n {
            trend_languages.truncate(top_n);
        }
        let trend_history = trend
            .filter(|report| report.history.len() >= 2)
            .map(|report| {
                report
                    .history
                    .iter()
                    .map(|point| HtmlHistoryPoint {
                        date: point.timestamp.format("%Y-%m-%d").to_string(),
                        label: point.label.clone().unwrap_or_default(),
                        code: point.code,
                        cyclomatic: point.cyclomatic,
                        files: point.files,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let html = CombinedHtmlReport {
            title: "Codelens - Code Analysis Report",
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            tool_version: env!("CARGO_PKG_VERSION"),
            summary: &result.summary,
            by_language,
            elapsed_secs: result.elapsed.as_secs_f64(),
            health_grade: health.grade.to_string(),
            health_score: health_view.score,
            health_is_scored: health.assessment == HealthAssessment::Scored,
            health_model: health_view.model,
            health_scope: health_view.scope,
            health_confidence: health_view.confidence,
            health_tail_score: health_view.tail_score,
            health_failing_files: health_view.failing_files,
            health_failing_ratio: health_view.failing_ratio,
            health_test: health_view.test_health.unwrap_or_default(),
            health_decision_label: health_decision.label,
            health_decision_tone: health_decision.tone,
            health_decision_summary: health_decision.summary,
            dimensions: health_view
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score_display.clone(),
                    grade: d.grade,
                    explanation: health_dimension_explanation(d.dimension).to_string(),
                })
                .collect(),
            health_languages: health_view
                .languages
                .iter()
                .map(html_language_health)
                .collect(),
            worst_directories: health_view
                .directories
                .iter()
                .map(html_directory_health)
                .collect(),
            worst_files,
            priority_files,
            show_history_sections,
            git_history_note: git_history_note.to_string(),
            hotspot_since: hotspot.map(|report| report.since.clone()).unwrap_or_default(),
            hotspot_total_commits: hotspot.map(|report| report.total_commits).unwrap_or(0),
            hotspots,
            coupling_since: coupling.map(|report| report.since.clone()).unwrap_or_default(),
            coupling_total_commits: coupling.map(|report| report.total_commits).unwrap_or(0),
            coupling_excluded_test_files: coupling
                .map(|report| report.excluded_test_files)
                .unwrap_or(0),
            coupling_pairs,
            trend_note: trend_note.to_string(),
            trend_available: trend.is_some(),
            trend_from_date: trend
                .map(|report| report.from.timestamp.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            trend_from_label: trend
                .and_then(|report| report.from.label.clone())
                .unwrap_or_default(),
            trend_to_date: trend
                .map(|report| report.to.timestamp.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            trend_metrics,
            trend_languages,
            trend_history,
            total_sloc: estimation.total_sloc,
            cost_scope: format!(
                "All {} counted SLOC across recognized files, including tests and counted documentation or configuration formats. Counted languages: {}.",
                estimation.total_sloc, counted_languages
            ),
            traditional_estimation_rows,
            locomo_estimation_rows,
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
        let health_view = HealthPresentation::from_report(report);
        let html = HealthHtmlReport {
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            tool_version: env!("CARGO_PKG_VERSION"),
            model: health_view.model,
            grade: report.grade.to_string(),
            score: health_view.score,
            is_scored: report.assessment == HealthAssessment::Scored,
            scope: health_view.scope,
            confidence: health_view.confidence,
            tail_score: health_view.tail_score,
            failing_files: health_view.failing_files,
            failing_ratio: health_view.failing_ratio,
            test_health: health_view.test_health.unwrap_or_default(),
            baseline_comparison: regression
                .map(|comparison| {
                    let summary = format!(
                        "{} using {}: {} ({:.1}) → {} ({:.1}), Δ {}",
                        comparison.baseline,
                        health_model_display_name(&comparison.model),
                        comparison.baseline_grade,
                        comparison.baseline_score,
                        comparison.current_grade,
                        comparison.current_score,
                        format_optional_delta(Some(comparison.score_delta)),
                    );
                    if comparison.assessment == HealthAssessment::InsufficientData {
                        format!("{summary}. Insufficient data; score delta is an estimate.")
                    } else {
                        summary
                    }
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
                    if comparison.assessment == HealthAssessment::InsufficientData {
                        "INSUFFICIENT DATA".to_string()
                    } else if comparison.failed {
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
            dimensions: health_view
                .dimensions
                .iter()
                .map(|d| HtmlDimensionScore {
                    dimension: d.dimension.to_string(),
                    score_display: d.score_display.clone(),
                    grade: d.grade,
                    explanation: health_dimension_explanation(d.dimension).to_string(),
                })
                .collect(),
            by_language: health_view
                .languages
                .iter()
                .map(html_language_health)
                .collect(),
            by_directory: health_view
                .directories
                .iter()
                .map(html_directory_health)
                .collect(),
            worst_files: health_view
                .worst_files
                .iter()
                .map(html_file_health)
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
            function_files: report
                .files
                .iter()
                .filter_map(|file| {
                    file.functions
                        .as_ref()
                        .map(|functions| HtmlFunctionHotspotFile {
                            path: file.path.display().to_string(),
                            functions: functions
                                .iter()
                                .map(|function| HtmlFunctionHotspot {
                                    name: function.name.clone(),
                                    start_line: function.start_line,
                                    end_line: function.end_line,
                                    commits: function.commits,
                                    cyclomatic: function.cyclomatic,
                                })
                                .collect(),
                        })
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
            focus: report.focus.as_ref().map(|path| path.display().to_string()),
            skipped_large_commits: report.skipped_large_commits,
            excluded_test_files: report.excluded_test_files,
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
            health_scope_change: if report.health.assessment == HealthAssessment::InsufficientData {
                "Insufficient data; health delta is an estimate.".to_string()
            } else if report.health.scope_changed {
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
    use crate::insight::estimation::{EstimationComparison, EstimationReport};
    use crate::insight::health::{
        Confidence, DimensionScore, DirectoryHealth, FileHealth, HealthReport, HealthScope,
        LanguageHealth, LanguageHealthChange, LanguageHealthChangeStatus, RegressionReport,
        TailRisk, TestHealth,
    };
    use crate::insight::scoring::default::DefaultModel;
    use crate::insight::scoring::HealthDimension;
    use crate::output::{OutputFormat, OutputOptions, Report};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn hotspot_html_preserves_function_breakdown() {
        use crate::analyzer::stats::Complexity;
        use crate::insight::hotspot::{
            ChurnMetrics, FileHotspot, FunctionHotspot, HotspotReport, RiskLevel,
        };

        let report = HotspotReport {
            files: vec![FileHotspot {
                path: PathBuf::from("src/lib.rs"),
                language: "Rust".into(),
                churn: ChurnMetrics {
                    commits: 3,
                    lines_added: 10,
                    lines_deleted: 2,
                    lines_churn: 12,
                },
                complexity: Complexity::default(),
                hotspot_score: 0.5,
                risk: RiskLevel::Medium,
                age_days: None,
                knowledge: None,
                functions: Some(vec![FunctionHotspot {
                    name: "analyze".into(),
                    start_line: 10,
                    end_line: 20,
                    commits: 3,
                    cyclomatic: 7,
                }]),
            }],
            since: "90d".into(),
            total_commits: 3,
        };
        let mut output = Vec::new();
        HtmlOutput::new()
            .write(
                &Report::Hotspot(report),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();
        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("Function Hotspots (approximate spans)"));
        assert!(html.contains("analyze"));
        assert!(html.contains("10-20"));
    }

    #[test]
    fn coupling_html_preserves_focus_and_filter_counts() {
        use crate::insight::coupling::CouplingReport;

        let report = CouplingReport {
            pairs: vec![],
            since: "90d".into(),
            total_commits: 4,
            skipped_large_commits: 2,
            excluded_test_files: 5,
            focus: Some(PathBuf::from("src/lib.rs")),
        };
        let mut output = Vec::new();
        HtmlOutput::new()
            .write(
                &Report::Coupling(report),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();
        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("Focused on: <strong>src/lib.rs</strong>"));
        assert!(html.contains("2 bulk commit(s) were excluded"));
        assert!(html.contains("5 test file(s) were excluded"));
    }

    fn representative_combined_report() -> crate::output::format::CombinedReport {
        let files = vec![FileStats {
            path: PathBuf::from("src/lib.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 120,
                code: 100,
                comment: 10,
                blank: 10,
            },
            size: 2_000,
            ..FileStats::default()
        }];
        let analysis = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(25),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };
        let health = HealthReport {
            score: 87.44,
            grade: Grade::B,
            model: "default-v2".to_string(),
            dimensions: vec![DimensionScore {
                dimension: HealthDimension::NestingDepth,
                score: 40.0,
                grade: Grade::F,
                weight: 0.1,
            }],
            by_directory: vec![DirectoryHealth {
                path: PathBuf::from("src/output"),
                score: 67.64,
                grade: Grade::D,
                file_count: 4,
            }],
            worst_files: vec![
                FileHealth {
                    path: PathBuf::from("src/output/markdown.rs"),
                    score: 43.13,
                    grade: Grade::F,
                    top_issue: HealthDimension::Complexity,
                    dimensions: vec![],
                },
                FileHealth {
                    path: PathBuf::from("src/output/console.rs"),
                    score: 43.63,
                    grade: Grade::F,
                    top_issue: HealthDimension::FileSize,
                    dimensions: vec![],
                },
                FileHealth {
                    path: PathBuf::from("src/analyzer/counter.rs"),
                    score: 68.85,
                    grade: Grade::D,
                    top_issue: HealthDimension::FuncSize,
                    dimensions: vec![],
                },
            ],
            scope: HealthScope::Production,
            confidence: Confidence::from_coverage(0.997),
            assessment: Default::default(),
            tail_risk: TailRisk {
                tail_score: 47.27,
                failing_files: 2,
                failing_ratio: 0.033,
                worst_score: 43.13,
            },
            by_language: vec![],
            test_health: Some(TestHealth {
                score: 91.25,
                grade: Grade::A,
                file_count: 12,
                code_lines: 800,
                confidence: Confidence::from_coverage(0.95),
                assessment: Default::default(),
                tail_risk: TailRisk::default(),
            }),
            regression: None,
        };
        let estimation = EstimationComparison {
            total_sloc: 100,
            reports: vec![
                EstimationReport {
                    model: "COCOMO Basic".to_string(),
                    total_sloc: 100,
                    effort_months: 1.0,
                    schedule_months: 1.0,
                    people_required: 1.0,
                    estimated_cost: 10_000.0,
                    by_language: vec![],
                    params: vec![
                        (
                            "Average annual salary".to_string(),
                            "$56,286.00".to_string(),
                        ),
                        (
                            "Management and facilities multiplier".to_string(),
                            "2.40".to_string(),
                        ),
                        ("Project Type".to_string(), "organic".to_string()),
                        ("EAF".to_string(), "1.00".to_string()),
                    ],
                },
                EstimationReport {
                    model: "LOCOMO".to_string(),
                    total_sloc: 100,
                    effort_months: 0.1,
                    schedule_months: 0.01,
                    people_required: 1.0,
                    estimated_cost: 1.25,
                    by_language: vec![],
                    params: vec![
                        (
                            "LLM Pricing".to_string(),
                            "In $3.00/Out $15.00 per 1M tokens".to_string(),
                        ),
                        ("TPS".to_string(), "50".to_string()),
                        ("Review".to_string(), "0.10 min/line".to_string()),
                    ],
                },
            ],
        };
        crate::output::format::CombinedReport {
            analysis,
            health,
            estimation,
        }
    }

    fn render_combined(combined: crate::output::format::CombinedReport) -> String {
        let mut output = Vec::new();
        HtmlOutput::new()
            .write(
                &Report::Combined(Box::new(combined)),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();
        String::from_utf8(output).unwrap()
    }

    fn render_combined_dashboard(combined: crate::output::format::CombinedReport) -> String {
        use crate::analyzer::stats::Complexity;
        use crate::insight::coupling::{CouplingPair, CouplingReport};
        use crate::insight::hotspot::{ChurnMetrics, FileHotspot, HotspotReport, RiskLevel};
        use crate::insight::trend::{HistoryPoint, SnapshotMeta, TrendReport};

        let hotspot = HotspotReport {
            files: vec![FileHotspot {
                path: PathBuf::from("src/lib.rs"),
                language: "Rust".to_string(),
                churn: ChurnMetrics {
                    commits: 12,
                    lines_added: 80,
                    lines_deleted: 20,
                    lines_churn: 100,
                },
                complexity: Complexity {
                    cyclomatic: 18,
                    ..Complexity::default()
                },
                hotspot_score: 0.72,
                risk: RiskLevel::High,
                age_days: Some(200),
                knowledge: None,
                functions: None,
            }],
            since: "90d".to_string(),
            total_commits: 24,
        };
        let coupling = CouplingReport {
            pairs: vec![CouplingPair {
                file_a: PathBuf::from("src/lib.rs"),
                file_b: PathBuf::from("src/output.rs"),
                shared_commits: 8,
                commits_a: 12,
                commits_b: 10,
                degree: 72.7,
            }],
            since: "90d".to_string(),
            total_commits: 24,
            skipped_large_commits: 0,
            excluded_test_files: 2,
            focus: None,
        };
        let (delta, by_language) = crate::insight::trend::compute_delta(
            &combined.analysis.summary,
            &combined.analysis.summary,
        );
        let now = chrono::Utc::now();
        let trend = TrendReport {
            from: SnapshotMeta {
                timestamp: now - chrono::Duration::days(7),
                label: Some("before".to_string()),
                git_commit: None,
                file_path: PathBuf::from("before.json"),
            },
            to: SnapshotMeta {
                timestamp: now,
                label: Some("Current".to_string()),
                git_commit: None,
                file_path: PathBuf::new(),
            },
            delta,
            by_language,
            history: vec![
                HistoryPoint {
                    timestamp: now - chrono::Duration::days(7),
                    label: Some("before".to_string()),
                    files: 1,
                    code: 90,
                    cyclomatic: 15,
                },
                HistoryPoint {
                    timestamp: now,
                    label: Some("Current".to_string()),
                    files: 1,
                    code: 100,
                    cyclomatic: 18,
                },
            ],
        };

        let mut output = Vec::new();
        HtmlOutput::new()
            .write_combined_dashboard(
                &combined,
                Some(&hotspot),
                Some(&coupling),
                Some(&trend),
                "",
                "",
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn combined_html_is_one_complete_document() {
        let html = render_combined(representative_combined_report());

        assert_eq!(html.matches("<!DOCTYPE html>").count(), 1);
        assert_eq!(html.matches("<html lang=\"en\">").count(), 1);
        assert_eq!(html.matches("<head>").count(), 1);
        assert_eq!(html.matches("<body>").count(), 1);
        assert_eq!(html.matches("</html>").count(), 1);
        assert!(html.contains("id=\"theme-toggle\""));
        assert!(html.contains("localStorage.getItem('codelens-theme')"));
        assert!(html.contains(":root[data-theme=\"dark\"]"));
        assert!(html.contains("prefers-color-scheme: dark"));
        assert_eq!(
            html.matches("href=\"https://github.com/DropFan/codelens\"")
                .count(),
            2
        );
    }

    #[test]
    fn combined_html_leads_with_project_overview_then_health_risk() {
        let html = render_combined(representative_combined_report());

        assert!(html.contains("Project Overview"));
        assert!(html.contains("Counted files"));
        assert!(html.contains("Code lines"));
        assert!(html.contains("Counted languages"));
        assert!(html.contains("Measured functions"));
        assert!(html.contains("Score: 87.4 / 100"));
        assert!(html.contains("Health model"));
        assert!(html.contains("Production code"));
        assert!(html.contains("Measurement confidence"));
        assert!(html.contains("F-grade files"));
        assert!(html.contains("Worst 10% file average"));
        assert!(html.contains("Worst directory"));
        assert!(html.contains("src/output"));
        assert!(html.contains("src/output/markdown.rs"));
        assert!(html.contains("src/output/console.rs"));
        assert!(html.contains("src/analyzer/counter.rs"));
        assert!(html.contains("Test code health"));
        assert!(html.contains("Needs attention"));
        assert!(html.contains("The health score is a current snapshot"));
        assert!(html.contains("Priority Checks"));
        assert!(html.contains("Why this file:"));
        assert!(html.contains("Suggested check:"));
        assert!(html.contains("Focus command:"));
        assert!(html.contains("How deeply conditions and loops are nested"));
        assert!(html.contains("How to Read This Report"));

        let overview = html.find("Project Overview").unwrap();
        let health = html.find("Health and Risk Summary").unwrap();
        let languages = html.find("Language Breakdown").unwrap();
        let cost = html.find("Cost Estimation").unwrap();
        assert!(overview < health);
        assert!(health < languages);
        assert!(languages < cost);
    }

    #[test]
    fn combined_dashboard_includes_git_and_snapshot_history() {
        let html = render_combined_dashboard(representative_combined_report());

        assert!(html.contains("Change Hotspots"));
        assert!(html.contains("src/lib.rs"));
        assert!(html.contains("Change Coupling"));
        assert!(html.contains("src/output.rs"));
        assert!(html.contains("Historical Change"));
        assert!(html.contains("Snapshot history"));
        assert!(html.contains("before"));
        assert!(html.contains("historyChart"));
    }

    #[test]
    fn combined_html_explains_cost_assumptions_and_survives_without_charts() {
        let html = render_combined(representative_combined_report());

        assert!(html.contains("Traditional workforce models"));
        assert!(html.contains("Average annual salary"));
        assert!(html.contains("Management and facilities multiplier"));
        assert!(html.contains("Project Type"));
        assert!(html.contains("EAF"));
        assert!(html.contains("LOCOMO estimates LLM generation cost"));
        assert!(html.contains("LLM Pricing"));
        assert!(html.contains("TPS"));
        assert!(html.contains("Review"));
        assert!(html.contains("typeof Chart"));
        assert!(
            html.contains("Charts are optional; all report data is available in text and tables.")
        );
    }

    #[test]
    fn combined_html_uses_a_precise_status_when_no_files_are_f_grade() {
        let mut combined = representative_combined_report();
        combined.health.tail_risk.failing_files = 0;
        combined.health.tail_risk.failing_ratio = 0.0;
        combined.health.worst_files.clear();

        let html = render_combined(combined);

        assert!(html.contains("No F-grade files detected"));
        assert!(html.contains("with no F-grade files among the evaluated files"));
    }

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
    fn shell_paths_are_quoted_for_copyable_focus_commands() {
        assert_eq!(shell_quote_for_display("src/output"), "'src/output'");
        assert_eq!(
            shell_quote_for_display("src/owner's output"),
            "'src/owner'\"'\"'s output'"
        );
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
            assessment: Default::default(),
            tail_risk: TailRisk::default(),
            by_language: vec![LanguageHealth {
                language: "Rust".to_string(),
                score: 82.0,
                grade: Grade::B,
                file_count: 4,
                code_lines: 300,
                confidence: Confidence::from_coverage(1.0),
                assessment: Default::default(),
                dimensions: vec![],
                tail_risk: TailRisk::default(),
            }],
            test_health: None,
            regression: Some(RegressionReport {
                baseline: "main".to_string(),
                model: "default-v2".to_string(),
                assessment: Default::default(),
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
                &Report::Health(report.clone()),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();

        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("id=\"theme-toggle\""));
        assert!(html.contains("localStorage.getItem('codelens-theme')"));
        assert!(html.contains(":root[data-theme=\"dark\"]"));
        assert_eq!(
            html.matches("href=\"https://github.com/DropFan/codelens\"")
                .count(),
            2
        );
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
    fn low_confidence_health_html_hides_the_letter_grade() {
        let files = vec![FileStats {
            path: PathBuf::from("src/custom.foo"),
            language: "Custom".to_string(),
            lines: LineStats {
                total: 100,
                code: 90,
                comment: 5,
                blank: 5,
            },
            size: 1_000,
            ..FileStats::default()
        }];
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::ZERO,
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };
        let report =
            crate::insight::health::score(&result, &DefaultModel::without_duplication(), 10);
        let mut output = Vec::new();

        HtmlOutput::new()
            .write(
                &Report::Health(report.clone()),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();

        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("class=\"assessment-badge\">Insufficient data"));
        assert!(html.contains("Estimated score:"));
        assert!(html.contains("Estimated 100"));
        assert!(!html.contains("class=\"grade-badge grade-A\""));

        let combined = crate::output::format::CombinedReport {
            analysis: result,
            health: report,
            estimation: crate::insight::estimation::EstimationComparison {
                total_sloc: 90,
                reports: Vec::new(),
            },
        };
        let mut output = Vec::new();
        HtmlOutput::new()
            .write(
                &Report::Combined(Box::new(combined)),
                &OutputOptions::default(),
                &mut output,
            )
            .unwrap();
        let html = String::from_utf8(output).unwrap();
        assert!(html.contains("Insufficient data"));
        assert!(html.contains("Estimated score:"));
        assert!(html.contains("Treat the 100.0 score as an estimate"));
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
