//! Code health score analysis.
//!
//! Computes health scores at three levels: project, directory, and file.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::analyzer::stats::{AnalysisResult, FileStats};
use crate::insight::scoring::{HealthDimension, HealthScoringFlow, RawMetrics, ScoringModel};
use crate::insight::Grade;

#[derive(Debug, Clone, Serialize)]
pub struct DimensionScore {
    pub dimension: HealthDimension,
    pub score: f64,
    pub grade: Grade,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileHealth {
    pub path: PathBuf,
    pub score: f64,
    pub grade: Grade,
    pub top_issue: HealthDimension,
    pub dimensions: Vec<DimensionScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryHealth {
    pub path: PathBuf,
    pub score: f64,
    pub grade: Grade,
    pub file_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthScope {
    Production,
    TestsOnly,
    LegacyAll,
    Empty,
}

impl std::fmt::Display for HealthScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Production => "production",
            Self::TestsOnly => "tests-only",
            Self::LegacyAll => "legacy-all",
            Self::Empty => "empty",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for ConfidenceLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Confidence {
    /// Fraction of the model's configured weight backed by measurements.
    pub coverage: f64,
    pub level: ConfidenceLevel,
}

impl Confidence {
    pub fn from_coverage(coverage: f64) -> Self {
        let coverage = if coverage.is_finite() {
            coverage.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let level = if coverage >= 0.80 {
            ConfidenceLevel::High
        } else if coverage >= 0.60 {
            ConfidenceLevel::Medium
        } else {
            ConfidenceLevel::Low
        };
        Self { coverage, level }
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::from_coverage(0.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TailRisk {
    pub tail_score: f64,
    pub failing_files: usize,
    pub failing_ratio: f64,
    pub worst_score: f64,
}

impl Default for TailRisk {
    fn default() -> Self {
        Self {
            tail_score: 0.0,
            failing_files: 0,
            failing_ratio: 0.0,
            worst_score: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageHealth {
    pub language: String,
    pub score: f64,
    pub grade: Grade,
    pub file_count: usize,
    pub code_lines: usize,
    pub confidence: Confidence,
    pub dimensions: Vec<DimensionScore>,
    pub tail_risk: TailRisk,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestHealth {
    pub score: f64,
    pub grade: Grade,
    pub file_count: usize,
    pub code_lines: usize,
    pub confidence: Confidence,
    pub tail_risk: TailRisk,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub score: f64,
    pub grade: Grade,
    pub model: String,
    pub dimensions: Vec<DimensionScore>,
    pub by_directory: Vec<DirectoryHealth>,
    pub worst_files: Vec<FileHealth>,
    pub scope: HealthScope,
    pub confidence: Confidence,
    pub tail_risk: TailRisk,
    pub by_language: Vec<LanguageHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_health: Option<TestHealth>,
    /// Delta against a baseline (--baseline); None for plain reports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regression: Option<RegressionReport>,
}

/// A file whose health grade dropped compared to the baseline.
#[derive(Debug, Clone, Serialize)]
pub struct FileRegression {
    pub path: PathBuf,
    /// True when the file did not exist in the baseline.
    pub is_new: bool,
    pub from_score: f64,
    pub from_grade: Grade,
    pub to_score: f64,
    pub to_grade: Grade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanguageHealthChangeStatus {
    Added,
    Removed,
    Changed,
    Unchanged,
}

impl std::fmt::Display for LanguageHealthChangeStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
            Self::Unchanged => "unchanged",
        })
    }
}

/// Per-language health movement under the comparison model.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageHealthChange {
    pub language: String,
    pub status: LanguageHealthChangeStatus,
    pub from_score: Option<f64>,
    pub from_grade: Option<Grade>,
    pub to_score: Option<f64>,
    pub to_grade: Option<Grade>,
    pub score_delta: Option<f64>,
}

/// Health delta against a baseline tree (snapshot or git ref).
///
/// Follows the "clean as you code" gate philosophy: a grade drop or a
/// five-point score drop fails, and newly-added F files also fail.
#[derive(Debug, Clone, Serialize)]
pub struct RegressionReport {
    /// Human-readable baseline reference (snapshot id or git ref).
    pub baseline: String,
    /// Scoring model used for both sides of this comparison.
    pub model: String,
    pub baseline_score: f64,
    pub baseline_grade: Grade,
    /// Main scoring scope on each side. A production/tests-only transition
    /// makes the project-level delta informational rather than gateable.
    pub baseline_scope: HealthScope,
    /// Current score recomputed with `model`; it can differ from the main
    /// report when only one side collected duplication data.
    pub current_score: f64,
    pub current_grade: Grade,
    pub current_scope: HealthScope,
    pub scope_changed: bool,
    /// Current score minus baseline score.
    pub score_delta: f64,
    /// Project letter grade dropped (e.g. B → C).
    pub project_regressed: bool,
    /// Existing files that regressed and newly-added F files, worst first.
    pub regressed_files: Vec<FileRegression>,
    /// Files present in both trees whose letter grade improved.
    pub improved_files: usize,
    /// Language health movement for explanation only. It does not trigger
    /// the regression gate in v2.
    pub by_language: Vec<LanguageHealthChange>,
    /// Gate verdict: project regressed or any file regressed.
    pub failed: bool,
}

/// Compare current analysis against a baseline tree with the same scoring
/// model. Paths are normalized by stripping a leading "./". Removed files
/// do not regress; newly-added F files do.
pub fn compare_with_baseline(
    baseline: &AnalysisResult,
    current: &AnalysisResult,
    model: &dyn ScoringModel,
    baseline_label: &str,
) -> RegressionReport {
    let (baseline, current) = comparable_results(baseline, current, model);
    let baseline_report = score(&baseline, model, usize::MAX);
    let current_report = score(&current, model, usize::MAX);
    let baseline_files: HashMap<PathBuf, (f64, Grade)> = baseline_report
        .worst_files
        .iter()
        .map(|file| (normalized_path(&file.path), (file.score, file.grade)))
        .collect();

    let mut regressed_files = Vec::new();
    let mut improved_files = 0usize;
    for file in &current_report.worst_files {
        let to_score = file.score;
        let to_grade = file.grade;
        let Some(&(from_score, from_grade)) = baseline_files.get(&normalized_path(&file.path))
        else {
            if to_grade == Grade::F {
                regressed_files.push(FileRegression {
                    path: file.path.clone(),
                    is_new: true,
                    from_score: 100.0,
                    from_grade: Grade::A,
                    to_score,
                    to_grade,
                });
            }
            continue;
        };
        // Grade orders A < B < ... < F, so "greater" means worse.
        if to_grade > from_grade || from_score - to_score >= 5.0 - 1e-9 {
            regressed_files.push(FileRegression {
                path: file.path.clone(),
                is_new: false,
                from_score,
                from_grade,
                to_score,
                to_grade,
            });
        } else if to_grade < from_grade || to_score - from_score >= 5.0 - 1e-9 {
            improved_files += 1;
        }
    }
    regressed_files.sort_by(|a, b| {
        b.to_grade.cmp(&a.to_grade).then(
            a.to_score
                .partial_cmp(&b.to_score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let baseline_score = baseline_report.score;
    let baseline_grade = baseline_report.grade;
    let baseline_scope = baseline_report.scope;
    let current_score = current_report.score;
    let current_grade = current_report.grade;
    let current_scope = current_report.scope;
    let scope_changed = matches!(
        (baseline_scope, current_scope),
        (HealthScope::Production, HealthScope::TestsOnly)
            | (HealthScope::TestsOnly, HealthScope::Production)
    );
    let project_regressed = !scope_changed
        && (current_grade > baseline_grade || baseline_score - current_score >= 5.0 - 1e-9);
    let by_language =
        language_health_changes(&baseline_report.by_language, &current_report.by_language);

    RegressionReport {
        baseline: baseline_label.to_string(),
        model: model.name().to_string(),
        baseline_score,
        baseline_grade,
        baseline_scope,
        current_score,
        current_grade,
        current_scope,
        scope_changed,
        score_delta: current_score - baseline_score,
        project_regressed,
        failed: project_regressed || !regressed_files.is_empty(),
        regressed_files,
        improved_files,
        by_language,
    }
}

/// Old snapshots predate explicit measurement flags. For files present on
/// both sides, compare only dimensions that both analyses actually measured.
/// If an entire side lacks one measurement kind, remove it from every file so
/// aggregate scores also use the same basis.
fn comparable_results(
    baseline: &AnalysisResult,
    current: &AnalysisResult,
    model: &dyn ScoringModel,
) -> (AnalysisResult, AnalysisResult) {
    let mut baseline = baseline.clone();
    let mut current = current.clone();
    if model.health_scoring_flow() != HealthScoringFlow::FileFirstV2 {
        return (baseline, current);
    }

    let functions_available = baseline
        .files
        .iter()
        .any(|file| file.complexity.functions_measured)
        && current
            .files
            .iter()
            .any(|file| file.complexity.functions_measured);
    let control_flow_available = baseline
        .files
        .iter()
        .any(|file| file.complexity.control_flow_measured)
        && current
            .files
            .iter()
            .any(|file| file.complexity.control_flow_measured);

    if !functions_available || !control_flow_available {
        for file in baseline.files.iter_mut().chain(current.files.iter_mut()) {
            if !functions_available {
                file.complexity.functions_measured = false;
            }
            if !control_flow_available {
                file.complexity.control_flow_measured = false;
            }
        }
    }

    let baseline_measurements: HashMap<PathBuf, (bool, bool)> = baseline
        .files
        .iter()
        .map(|file| {
            (
                normalized_path(&file.path),
                (
                    file.complexity.functions_measured,
                    file.complexity.control_flow_measured,
                ),
            )
        })
        .collect();
    let current_measurements: HashMap<PathBuf, (bool, bool)> = current
        .files
        .iter()
        .map(|file| {
            (
                normalized_path(&file.path),
                (
                    file.complexity.functions_measured,
                    file.complexity.control_flow_measured,
                ),
            )
        })
        .collect();

    for file in &mut baseline.files {
        if let Some(&(functions, control_flow)) =
            current_measurements.get(&normalized_path(&file.path))
        {
            file.complexity.functions_measured &= functions;
            file.complexity.control_flow_measured &= control_flow;
        }
    }
    for file in &mut current.files {
        if let Some(&(functions, control_flow)) =
            baseline_measurements.get(&normalized_path(&file.path))
        {
            file.complexity.functions_measured &= functions;
            file.complexity.control_flow_measured &= control_flow;
        }
    }

    (baseline, current)
}

fn language_health_changes(
    baseline: &[LanguageHealth],
    current: &[LanguageHealth],
) -> Vec<LanguageHealthChange> {
    let baseline: BTreeMap<&str, &LanguageHealth> = baseline
        .iter()
        .map(|language| (language.language.as_str(), language))
        .collect();
    let current: BTreeMap<&str, &LanguageHealth> = current
        .iter()
        .map(|language| (language.language.as_str(), language))
        .collect();
    let mut names: Vec<&str> = baseline.keys().chain(current.keys()).copied().collect();
    names.sort_unstable();
    names.dedup();

    names
        .into_iter()
        .map(|language| {
            let from = baseline.get(language).copied();
            let to = current.get(language).copied();
            let score_delta = from.zip(to).map(|(from, to)| to.score - from.score);
            let status = match (from, to) {
                (None, Some(_)) => LanguageHealthChangeStatus::Added,
                (Some(_), None) => LanguageHealthChangeStatus::Removed,
                (Some(from), Some(to))
                    if from.grade != to.grade || (to.score - from.score).abs() >= 0.05 =>
                {
                    LanguageHealthChangeStatus::Changed
                }
                (Some(_), Some(_)) => LanguageHealthChangeStatus::Unchanged,
                (None, None) => unreachable!("language name came from neither report"),
            };
            LanguageHealthChange {
                language: language.to_string(),
                status,
                from_score: from.map(|health| health.score),
                from_grade: from.map(|health| health.grade),
                to_score: to.map(|health| health.score),
                to_grade: to.map(|health| health.grade),
                score_delta,
            }
        })
        .collect()
}

/// Generate a health report from analysis results using the given scoring model.
pub fn score(result: &AnalysisResult, model: &dyn ScoringModel, top_n: usize) -> HealthReport {
    match model.health_scoring_flow() {
        HealthScoringFlow::Legacy => score_legacy(result, model, top_n),
        HealthScoringFlow::FileFirstV2 => score_v2(result, model, top_n),
    }
}

#[cfg(test)]
fn score_file(file: &FileStats, model: &dyn ScoringModel) -> FileHealth {
    match model.health_scoring_flow() {
        HealthScoringFlow::Legacy => score_file_legacy(file, model),
        HealthScoringFlow::FileFirstV2 => score_file_v2(file, model).health,
    }
}

fn score_legacy(result: &AnalysisResult, model: &dyn ScoringModel, top_n: usize) -> HealthReport {
    if result.files.is_empty() {
        return empty_report(model);
    }

    let files: Vec<&FileStats> = result.files.iter().collect();
    let mut scored: Vec<ScoredFile<'_>> = files
        .iter()
        .map(|file| ScoredFile {
            file,
            health: score_file_legacy(file, model),
            confidence: measurement_coverage(file, model),
        })
        .collect();
    scored.sort_by(file_score_order);

    let mut directories: HashMap<PathBuf, Vec<&FileStats>> = HashMap::new();
    for file in &files {
        directories
            .entry(normalized_parent(&file.path))
            .or_default()
            .push(*file);
    }
    let mut by_directory: Vec<DirectoryHealth> = directories
        .into_iter()
        .map(|(path, files)| {
            let metrics = RawMetrics::from_file_refs_legacy(&files);
            let score = model.total_score(&metrics);
            DirectoryHealth {
                path,
                score,
                grade: model.grade(score),
                file_count: files.len(),
            }
        })
        .collect();
    by_directory.sort_by(directory_score_order);

    let metrics = RawMetrics::from_file_refs_legacy(&files);
    let project_score = model.total_score(&metrics);
    let scored_refs: Vec<&ScoredFile<'_>> = scored.iter().collect();
    let by_language = legacy_language_healths(&scored_refs, model);
    let test_files: Vec<&ScoredFile<'_>> = scored_refs
        .iter()
        .copied()
        .filter(|file| crate::analyzer::test_code::is_test_path(&file.file.path))
        .collect();

    HealthReport {
        score: project_score,
        grade: model.grade(project_score),
        model: model.name().to_string(),
        dimensions: score_dimensions(&metrics, model),
        by_directory: by_directory.into_iter().take(top_n).collect(),
        worst_files: scored
            .iter()
            .take(top_n)
            .map(|file| file.health.clone())
            .collect(),
        scope: HealthScope::LegacyAll,
        confidence: aggregate_confidence(&scored_refs),
        tail_risk: tail_risk(&scored_refs),
        by_language,
        test_health: test_health(&test_files, model, true),
        regression: None,
    }
}

fn score_v2(result: &AnalysisResult, model: &dyn ScoringModel, top_n: usize) -> HealthReport {
    let code = code_files(&result.files);
    if code.is_empty() {
        return empty_report(model);
    }

    let production: Vec<&FileStats> = code
        .iter()
        .copied()
        .filter(|file| !crate::analyzer::test_code::is_test_path(&file.path))
        .collect();
    let tests: Vec<&FileStats> = code
        .iter()
        .copied()
        .filter(|file| crate::analyzer::test_code::is_test_path(&file.path))
        .collect();
    let (scope, primary) = if production.is_empty() {
        (HealthScope::TestsOnly, tests.as_slice())
    } else {
        (HealthScope::Production, production.as_slice())
    };

    let mut scored: Vec<ScoredFile<'_>> = primary
        .iter()
        .map(|file| score_file_v2(file, model))
        .collect();
    scored.sort_by(file_score_order);
    let scored_refs: Vec<&ScoredFile<'_>> = scored.iter().collect();
    let by_language = language_healths(&scored_refs, model);
    let (project_score, project_confidence) = language_weighted_summary(&by_language);
    let mut by_directory = directory_healths(&scored_refs, model);
    by_directory.sort_by(directory_score_order);

    let test_scored: Vec<ScoredFile<'_>> = tests
        .iter()
        .map(|file| score_file_v2(file, model))
        .collect();
    let test_refs: Vec<&ScoredFile<'_>> = test_scored.iter().collect();

    HealthReport {
        score: project_score,
        grade: model.grade(project_score),
        model: model.name().to_string(),
        dimensions: aggregate_dimensions(&scored_refs, model),
        by_directory: by_directory.into_iter().take(top_n).collect(),
        worst_files: scored
            .iter()
            .take(top_n)
            .map(|file| file.health.clone())
            .collect(),
        scope,
        confidence: project_confidence,
        tail_risk: tail_risk(&scored_refs),
        by_language,
        test_health: test_health(&test_refs, model, false),
        regression: None,
    }
}

fn empty_report(model: &dyn ScoringModel) -> HealthReport {
    HealthReport {
        score: 0.0,
        grade: Grade::F,
        model: model.name().to_string(),
        dimensions: Vec::new(),
        by_directory: Vec::new(),
        worst_files: Vec::new(),
        scope: HealthScope::Empty,
        confidence: Confidence::from_coverage(0.0),
        tail_risk: TailRisk::default(),
        by_language: Vec::new(),
        test_health: None,
        regression: None,
    }
}

#[derive(Clone)]
struct ScoredFile<'a> {
    file: &'a FileStats,
    health: FileHealth,
    confidence: f64,
}

fn score_file_legacy(file: &FileStats, model: &dyn ScoringModel) -> FileHealth {
    let metrics = RawMetrics::from_file_legacy(file);
    let dimensions = score_dimensions(&metrics, model);
    let total = model.total_score(&metrics);

    let top_issue = dimensions
        .iter()
        .min_by(|a, b| a.score.total_cmp(&b.score))
        .map(|d| d.dimension)
        .unwrap_or(HealthDimension::Complexity);

    FileHealth {
        path: file.path.clone(),
        score: total,
        grade: model.grade(total),
        top_issue,
        dimensions,
    }
}

#[derive(Clone, Copy)]
struct MeasuredDimension {
    dimension: HealthDimension,
    score: f64,
    weight: f64,
}

fn score_file_v2<'a>(file: &'a FileStats, model: &dyn ScoringModel) -> ScoredFile<'a> {
    let mut metrics = RawMetrics::from_file(file);
    let branch_points = file
        .complexity
        .cyclomatic
        .saturating_sub(file.complexity.functions);
    let size_units = file.lines.code.div_ceil(50);
    let effective_units = file.complexity.functions.max(size_units).max(1);
    metrics.avg_cyclomatic = 1.0 + branch_points as f64 / effective_units as f64;

    let measured: Vec<MeasuredDimension> = model
        .dimensions()
        .iter()
        .filter(|weight| dimension_is_measured(file, weight.dimension))
        .filter_map(|weight| {
            let score = model.score_dimension(weight.dimension, &metrics);
            score.is_finite().then_some(MeasuredDimension {
                dimension: weight.dimension,
                score: score.clamp(0.0, 100.0),
                weight: weight.weight,
            })
        })
        .collect();
    let measured_weight: f64 = measured.iter().map(|dimension| dimension.weight).sum();
    let configured_weight: f64 = model
        .dimensions()
        .iter()
        .map(|dimension| dimension.weight)
        .sum();
    let total = combine_v2_dimensions(&measured);
    let top_issue = measured
        .iter()
        .enumerate()
        .max_by(|(left_index, _), (right_index, _)| {
            let left_gain = restored_gain(&measured, *left_index, total);
            let right_gain = restored_gain(&measured, *right_index, total);
            left_gain.total_cmp(&right_gain)
        })
        .map(|(_, dimension)| dimension.dimension)
        .unwrap_or(HealthDimension::FileSize);
    let dimensions = measured
        .iter()
        .map(|dimension| DimensionScore {
            dimension: dimension.dimension,
            score: dimension.score,
            grade: model.grade(dimension.score),
            weight: if measured_weight > 0.0 {
                dimension.weight / measured_weight
            } else {
                0.0
            },
        })
        .collect();

    ScoredFile {
        file,
        health: FileHealth {
            path: file.path.clone(),
            score: total,
            grade: model.grade(total),
            top_issue,
            dimensions,
        },
        confidence: if configured_weight > 0.0 {
            measured_weight / configured_weight
        } else {
            0.0
        },
    }
}

fn dimension_is_measured(file: &FileStats, dimension: HealthDimension) -> bool {
    match dimension {
        HealthDimension::Complexity | HealthDimension::NestingDepth => {
            file.complexity.control_flow_measured
        }
        HealthDimension::FuncSize => {
            file.complexity.functions_measured && file.complexity.functions > 0
        }
        HealthDimension::CommentRatio | HealthDimension::FileSize => true,
        HealthDimension::Duplication => true,
    }
}

fn measurement_coverage(file: &FileStats, model: &dyn ScoringModel) -> f64 {
    let configured: f64 = model
        .dimensions()
        .iter()
        .map(|dimension| dimension.weight)
        .sum();
    if configured == 0.0 {
        return 0.0;
    }
    model
        .dimensions()
        .iter()
        .filter(|dimension| dimension_is_measured(file, dimension.dimension))
        .map(|dimension| dimension.weight)
        .sum::<f64>()
        / configured
}

fn combine_v2_dimensions(dimensions: &[MeasuredDimension]) -> f64 {
    let total_weight: f64 = dimensions.iter().map(|dimension| dimension.weight).sum();
    if total_weight == 0.0 {
        return 0.0;
    }
    let base_score = dimensions
        .iter()
        .map(|dimension| dimension.score * dimension.weight)
        .sum::<f64>()
        / total_weight;
    let core_floor = dimensions
        .iter()
        .filter(|dimension| {
            matches!(
                dimension.dimension,
                HealthDimension::Complexity
                    | HealthDimension::FuncSize
                    | HealthDimension::FileSize
                    | HealthDimension::Duplication
            )
        })
        .map(|dimension| dimension.score)
        .min_by(f64::total_cmp)
        .unwrap_or(base_score);
    (0.85 * base_score + 0.15 * core_floor).clamp(0.0, 100.0)
}

fn restored_gain(dimensions: &[MeasuredDimension], index: usize, current: f64) -> f64 {
    let mut restored = dimensions.to_vec();
    restored[index].score = 100.0;
    combine_v2_dimensions(&restored) - current
}

fn score_dimensions(metrics: &RawMetrics, model: &dyn ScoringModel) -> Vec<DimensionScore> {
    // `total_score` divides by the weight sum, so serialized weights are
    // normalized the same way: they always describe each dimension's real
    // share of the total, even for models whose raw weights don't sum to 1
    // (e.g. the without-duplication variant).
    let total_weight: f64 = model.dimensions().iter().map(|d| d.weight).sum();
    model
        .dimensions()
        .iter()
        .map(|dw| {
            let s = model.score_dimension(dw.dimension, metrics);
            DimensionScore {
                dimension: dw.dimension,
                score: s,
                grade: model.grade(s),
                weight: if total_weight > 0.0 {
                    dw.weight / total_weight
                } else {
                    0.0
                },
            }
        })
        .collect()
}

const NON_CODE_LANGUAGES: &[&str] = &[
    "html",
    "css",
    "scss",
    "sass",
    "less",
    "json",
    "yaml",
    "toml",
    "xml",
    "ini",
    "csv",
    "markdown",
    "restructuredtext",
    "asciidoc",
    "latex",
];

fn code_files(files: &[FileStats]) -> Vec<&FileStats> {
    let mut seen = HashSet::new();
    files
        .iter()
        .filter(|file| {
            !NON_CODE_LANGUAGES
                .iter()
                .any(|language| file.language.eq_ignore_ascii_case(language))
                && seen.insert(normalized_path(&file.path))
        })
        .collect()
}

fn normalized_path(path: &Path) -> PathBuf {
    path.strip_prefix("./").unwrap_or(path).to_path_buf()
}

fn file_score_order(left: &ScoredFile<'_>, right: &ScoredFile<'_>) -> std::cmp::Ordering {
    left.health
        .score
        .total_cmp(&right.health.score)
        .then_with(|| left.health.path.cmp(&right.health.path))
}

fn directory_score_order(left: &DirectoryHealth, right: &DirectoryHealth) -> std::cmp::Ordering {
    left.score
        .total_cmp(&right.score)
        .then_with(|| left.path.cmp(&right.path))
}

fn file_weight(file: &FileStats) -> f64 {
    file.lines.code.max(1) as f64
}

fn center_weight(file: &FileStats) -> f64 {
    file_weight(file).sqrt()
}

fn weighted_average<I>(values: I) -> f64
where
    I: IntoIterator<Item = (f64, f64)>,
{
    let (sum, weight) = values
        .into_iter()
        .fold((0.0, 0.0), |(sum, total), (value, item_weight)| {
            (sum + value * item_weight, total + item_weight)
        });
    if weight > 0.0 {
        sum / weight
    } else {
        0.0
    }
}

fn tail_risk(files: &[&ScoredFile<'_>]) -> TailRisk {
    if files.is_empty() {
        return TailRisk::default();
    }
    let mut scores: Vec<f64> = files.iter().map(|file| file.health.score).collect();
    scores.sort_by(f64::total_cmp);
    let tail_count = scores.len().div_ceil(10).max(1);
    let tail_score = scores.iter().take(tail_count).sum::<f64>() / tail_count as f64;
    let failing_files = scores.iter().filter(|score| **score < 60.0).count();
    TailRisk {
        tail_score,
        failing_files,
        failing_ratio: failing_files as f64 / scores.len() as f64,
        worst_score: scores[0],
    }
}

fn aggregate_confidence(files: &[&ScoredFile<'_>]) -> Confidence {
    Confidence::from_coverage(weighted_average(
        files
            .iter()
            .map(|file| (file.confidence, center_weight(file.file))),
    ))
}

fn aggregate_dimensions(
    files: &[&ScoredFile<'_>],
    model: &dyn ScoringModel,
) -> Vec<DimensionScore> {
    let mut dimensions = Vec::new();
    for configured in model.dimensions() {
        let matching: Vec<(&ScoredFile<'_>, &DimensionScore)> = files
            .iter()
            .filter_map(|file| {
                file.health
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.dimension == configured.dimension)
                    .map(|dimension| (*file, dimension))
            })
            .collect();
        if matching.is_empty() {
            continue;
        }
        let score = weighted_average(
            matching
                .iter()
                .map(|(file, dimension)| (dimension.score, center_weight(file.file))),
        );
        let effective_weight = weighted_average(
            matching
                .iter()
                .map(|(file, dimension)| (dimension.weight, center_weight(file.file))),
        );
        dimensions.push(DimensionScore {
            dimension: configured.dimension,
            score,
            grade: model.grade(score),
            weight: effective_weight,
        });
    }
    let total_weight: f64 = dimensions.iter().map(|dimension| dimension.weight).sum();
    if total_weight > 0.0 {
        for dimension in &mut dimensions {
            dimension.weight /= total_weight;
        }
    }
    dimensions
}

fn aggregate_file_scores(files: &[&ScoredFile<'_>]) -> f64 {
    if files.is_empty() {
        return 0.0;
    }
    let center = weighted_average(
        files
            .iter()
            .map(|file| (file.health.score, center_weight(file.file))),
    );
    0.85 * center + 0.15 * tail_risk(files).tail_score
}

fn language_healths(files: &[&ScoredFile<'_>], model: &dyn ScoringModel) -> Vec<LanguageHealth> {
    let mut groups: BTreeMap<&str, Vec<&ScoredFile<'_>>> = BTreeMap::new();
    for file in files {
        groups.entry(&file.file.language).or_default().push(*file);
    }
    let mut languages: Vec<LanguageHealth> = groups
        .into_iter()
        .map(|(language, files)| {
            let score = aggregate_file_scores(&files);
            LanguageHealth {
                language: language.to_string(),
                score,
                grade: model.grade(score),
                file_count: files.len(),
                code_lines: files.iter().map(|file| file.file.lines.code).sum(),
                confidence: aggregate_confidence(&files),
                dimensions: aggregate_dimensions(&files, model),
                tail_risk: tail_risk(&files),
            }
        })
        .collect();
    languages.sort_by(|left, right| {
        right
            .code_lines
            .cmp(&left.code_lines)
            .then_with(|| left.language.cmp(&right.language))
    });
    languages
}

fn legacy_language_healths(
    files: &[&ScoredFile<'_>],
    model: &dyn ScoringModel,
) -> Vec<LanguageHealth> {
    let mut groups: BTreeMap<&str, Vec<&ScoredFile<'_>>> = BTreeMap::new();
    for file in files {
        groups.entry(&file.file.language).or_default().push(*file);
    }
    let mut languages: Vec<LanguageHealth> = groups
        .into_iter()
        .map(|(language, scored)| {
            let raw_files: Vec<&FileStats> = scored.iter().map(|file| file.file).collect();
            let metrics = RawMetrics::from_file_refs_legacy(&raw_files);
            let score = model.total_score(&metrics);
            LanguageHealth {
                language: language.to_string(),
                score,
                grade: model.grade(score),
                file_count: scored.len(),
                code_lines: scored.iter().map(|file| file.file.lines.code).sum(),
                confidence: aggregate_confidence(&scored),
                dimensions: score_dimensions(&metrics, model),
                tail_risk: tail_risk(&scored),
            }
        })
        .collect();
    languages.sort_by(|left, right| {
        right
            .code_lines
            .cmp(&left.code_lines)
            .then_with(|| left.language.cmp(&right.language))
    });
    languages
}

fn language_weighted_summary(languages: &[LanguageHealth]) -> (f64, Confidence) {
    let total_code: usize = languages.iter().map(|language| language.code_lines).sum();
    let weight = |language: &LanguageHealth| {
        if total_code > 0 {
            language.code_lines as f64
        } else {
            language.file_count as f64
        }
    };
    let score = weighted_average(
        languages
            .iter()
            .map(|language| (language.score, weight(language))),
    );
    let confidence = weighted_average(
        languages
            .iter()
            .map(|language| (language.confidence.coverage, weight(language))),
    );
    (score, Confidence::from_coverage(confidence))
}

fn directory_healths(files: &[&ScoredFile<'_>], model: &dyn ScoringModel) -> Vec<DirectoryHealth> {
    if files.is_empty() {
        return Vec::new();
    }
    let raw_files: Vec<&FileStats> = files.iter().map(|file| file.file).collect();
    let root = common_parent(&raw_files);
    let mut directories: HashMap<PathBuf, Vec<&ScoredFile<'_>>> = HashMap::new();
    for file in files {
        let mut directory = normalized_parent(&file.file.path);
        loop {
            directories
                .entry(directory.clone())
                .or_default()
                .push(*file);
            if directory == root {
                break;
            }
            let parent = normalized_parent(&directory);
            if parent == directory {
                break;
            }
            directory = parent;
        }
    }

    directories
        .into_iter()
        .map(|(path, files)| {
            let languages = language_healths(&files, model);
            let (score, _) = language_weighted_summary(&languages);
            DirectoryHealth {
                path,
                score,
                grade: model.grade(score),
                file_count: files.len(),
            }
        })
        .collect()
}

fn test_health(
    files: &[&ScoredFile<'_>],
    model: &dyn ScoringModel,
    legacy: bool,
) -> Option<TestHealth> {
    if files.is_empty() {
        return None;
    }
    let score = if legacy {
        let raw_files: Vec<&FileStats> = files.iter().map(|file| file.file).collect();
        model.total_score(&RawMetrics::from_file_refs_legacy(&raw_files))
    } else {
        let languages = language_healths(files, model);
        language_weighted_summary(&languages).0
    };
    Some(TestHealth {
        score,
        grade: model.grade(score),
        file_count: files.len(),
        code_lines: files.iter().map(|file| file.file.lines.code).sum(),
        confidence: aggregate_confidence(files),
        tail_risk: tail_risk(files),
    })
}

fn normalized_parent(path: &Path) -> PathBuf {
    let path = normalized_path(path);
    let parent = path.parent().unwrap_or(Path::new("."));
    if parent.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        parent.to_path_buf()
    }
}

fn common_parent(files: &[&FileStats]) -> PathBuf {
    let mut common = normalized_parent(&files[0].path);
    for file in &files[1..] {
        let parent = normalized_parent(&file.path);
        while !parent.starts_with(&common) {
            let next = normalized_parent(&common);
            if next == common {
                return common;
            }
            common = next;
        }
    }
    common
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{Complexity, FileStats, LineStats, Summary};
    use crate::insight::scoring::default::DefaultModel;
    use crate::insight::scoring::legacy::LegacyModel;
    use crate::insight::scoring::{DimensionWeight, HealthScoringFlow};
    use std::time::Duration;

    fn make_test_result() -> AnalysisResult {
        let files = vec![
            FileStats {
                path: PathBuf::from("src/good.rs"),
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
                    functions: 3,
                    cyclomatic: 6,
                    cognitive: 0,
                    max_depth: 2,
                    avg_func_lines: 13.0,
                    ..Complexity::default()
                },
            },
            FileStats {
                path: PathBuf::from("src/bad.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 500,
                    code: 400,
                    comment: 10,
                    blank: 90,
                },
                size: 10000,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions_measured: true,
                    control_flow_measured: true,
                    functions: 2,
                    cyclomatic: 30,
                    cognitive: 0,
                    max_depth: 8,
                    avg_func_lines: 200.0,
                    ..Complexity::default()
                },
            },
            FileStats {
                path: PathBuf::from("lib/utils.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 80,
                    code: 60,
                    comment: 10,
                    blank: 10,
                },
                size: 1500,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions_measured: true,
                    control_flow_measured: true,
                    functions: 5,
                    cyclomatic: 10,
                    cognitive: 0,
                    max_depth: 3,
                    avg_func_lines: 12.0,
                    ..Complexity::default()
                },
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(50),
            scanned_files: 3,
            skipped_files: 0,
            error_files: 0,
        }
    }

    const FILE_LINE_DIMENSION: &[DimensionWeight] = &[DimensionWeight {
        dimension: HealthDimension::FileSize,
        weight: 1.0,
    }];

    struct FileLineScoreModel;

    impl ScoringModel for FileLineScoreModel {
        fn name(&self) -> &str {
            "test-file-lines"
        }

        fn dimensions(&self) -> &[DimensionWeight] {
            FILE_LINE_DIMENSION
        }

        fn score_dimension(&self, _: HealthDimension, metrics: &RawMetrics) -> f64 {
            metrics.avg_file_lines
        }
    }

    const DIRECT_SCORE_DIMENSION: &[DimensionWeight] = &[DimensionWeight {
        dimension: HealthDimension::FuncSize,
        weight: 1.0,
    }];

    struct DirectScoreV2Model;

    impl ScoringModel for DirectScoreV2Model {
        fn name(&self) -> &str {
            "test-v2"
        }

        fn dimensions(&self) -> &[DimensionWeight] {
            DIRECT_SCORE_DIMENSION
        }

        fn score_dimension(&self, _: HealthDimension, metrics: &RawMetrics) -> f64 {
            metrics.avg_func_lines
        }

        fn health_scoring_flow(&self) -> HealthScoringFlow {
            HealthScoringFlow::FileFirstV2
        }
    }

    struct LegacyDepthModel;

    impl ScoringModel for LegacyDepthModel {
        fn name(&self) -> &str {
            "test-legacy-depth"
        }

        fn dimensions(&self) -> &[DimensionWeight] {
            const DIMENSIONS: &[DimensionWeight] = &[DimensionWeight {
                dimension: HealthDimension::NestingDepth,
                weight: 1.0,
            }];
            DIMENSIONS
        }

        fn score_dimension(&self, _: HealthDimension, metrics: &RawMetrics) -> f64 {
            metrics.depth as f64
        }
    }

    fn result_with_file_score(path: &str, score: usize) -> AnalysisResult {
        let files = vec![FileStats {
            path: PathBuf::from(path),
            language: "Rust".to_string(),
            lines: LineStats {
                total: score,
                code: score,
                comment: 0,
                blank: 0,
            },
            size: score as u64,
            duplicate_lines: 0,
            complexity: Complexity::default(),
        }];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::ZERO,
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        }
    }

    fn result_with_direct_scores(entries: &[(&str, &str, usize)]) -> AnalysisResult {
        let files = entries
            .iter()
            .map(|(path, language, score)| FileStats {
                path: PathBuf::from(path),
                language: (*language).to_string(),
                lines: LineStats {
                    total: 200,
                    code: 100,
                    comment: 0,
                    blank: 0,
                },
                size: 1_000,
                duplicate_lines: 0,
                complexity: Complexity {
                    functions_measured: true,
                    functions: 1,
                    avg_func_lines: *score as f64,
                    ..Complexity::default()
                },
            })
            .collect::<Vec<_>>();
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            scanned_files: files.len(),
            files,
            elapsed: Duration::ZERO,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn legacy_model_reproduces_historical_non_empty_score() {
        let report = score(&make_test_result(), &LegacyModel::new(), 10);

        assert_eq!(report.model, "default");
        assert_eq!(report.scope, HealthScope::LegacyAll);
        assert!((report.score - 80.153_333).abs() < 0.01, "{}", report.score);
    }

    #[test]
    fn legacy_flow_uses_legacy_metrics_at_every_report_level() {
        let files = vec![FileStats {
            path: PathBuf::from("tests/legacy.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 10,
                code: 8,
                comment: 1,
                blank: 1,
            },
            size: 100,
            duplicate_lines: 0,
            complexity: Complexity {
                max_depth: 1,
                legacy_max_depth: Some(9),
                ..Complexity::default()
            },
        }];
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::ZERO,
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };

        let report = score(&result, &LegacyDepthModel, 10);

        assert_eq!(report.score, 9.0);
        assert_eq!(report.dimensions[0].score, 9.0);
        assert_eq!(report.by_directory[0].score, 9.0);
        assert_eq!(report.worst_files[0].score, 9.0);
        assert_eq!(report.by_language[0].score, 9.0);
        assert_eq!(report.test_health.unwrap().score, 9.0);
    }

    #[test]
    fn v2_tail_weight_scales_with_bad_file_prevalence() {
        let mut hundred = vec![("src/bad.rs", "Rust", 30)];
        let good_names: Vec<String> = (0..99)
            .map(|index| format!("src/good-{index}.rs"))
            .collect();
        for name in &good_names {
            hundred.push((name.as_str(), "Rust", 90));
        }
        let report = score(
            &result_with_direct_scores(&hundred),
            &DirectScoreV2Model,
            200,
        );
        assert!((report.score - 88.59).abs() < 0.01, "{}", report.score);

        let mut ten = vec![("src/bad.rs", "Rust", 30)];
        let names: Vec<String> = (0..9).map(|index| format!("src/ok-{index}.rs")).collect();
        for name in &names {
            ten.push((name.as_str(), "Rust", 90));
        }
        let report = score(&result_with_direct_scores(&ten), &DirectScoreV2Model, 20);
        assert!((report.score - 75.9).abs() < 0.01, "{}", report.score);
    }

    #[test]
    fn v2_separates_production_and_test_health() {
        let result = result_with_direct_scores(&[
            ("src/lib.rs", "Rust", 90),
            ("tests/regression.rs", "Rust", 30),
        ]);
        let report = score(&result, &DirectScoreV2Model, 10);

        assert_eq!(report.scope, HealthScope::Production);
        assert!((report.score - 90.0).abs() < 0.01);
        let tests = report.test_health.unwrap();
        assert!((tests.score - 30.0).abs() < 0.01);
        assert_eq!(tests.file_count, 1);
    }

    #[test]
    fn v2_uses_tests_as_main_scope_only_when_no_production_exists() {
        let result = result_with_direct_scores(&[("tests/regression.rs", "Rust", 30)]);
        let report = score(&result, &DirectScoreV2Model, 10);

        assert_eq!(report.scope, HealthScope::TestsOnly);
        assert!((report.score - 30.0).abs() < 0.01);
    }

    #[test]
    fn v2_project_score_weights_language_scores_by_code_lines() {
        let mut result = result_with_direct_scores(&[
            ("src/main.rs", "Rust", 90),
            ("src/helper.ts", "TypeScript", 30),
        ]);
        result.files[0].lines.code = 900;
        result.files[1].lines.code = 100;

        let report = score(&result, &DirectScoreV2Model, 10);

        assert!((report.score - 84.0).abs() < 0.01, "{}", report.score);
        assert_eq!(report.by_language.len(), 2);
    }

    #[test]
    fn v2_aggregate_scores_stay_within_their_file_range() {
        let result = result_with_direct_scores(&[
            ("src/a.rs", "Rust", 30),
            ("src/nested/b.rs", "Rust", 60),
            ("lib/c.ts", "TypeScript", 90),
        ]);

        let report = score(&result, &DirectScoreV2Model, 20);

        assert!((30.0..=90.0).contains(&report.score));
        assert!(report
            .by_language
            .iter()
            .all(|language| (30.0..=90.0).contains(&language.score)));
        assert!(report
            .by_directory
            .iter()
            .all(|directory| (30.0..=90.0).contains(&directory.score)));
    }

    #[test]
    fn v2_omits_unmeasured_dimensions_and_reports_low_confidence() {
        let result = result_with_file_score("src/custom.foo", 100);
        let report = score(&result, &DefaultModel::without_duplication(), 10);

        assert_eq!(report.confidence.level, ConfidenceLevel::Low);
        assert!(report.worst_files[0]
            .dimensions
            .iter()
            .all(|dimension| matches!(
                dimension.dimension,
                HealthDimension::CommentRatio | HealthDimension::FileSize
            )));
    }

    #[test]
    fn test_health_report_structure() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.model, "default-v2");
        assert!(!report.dimensions.is_empty());
        assert!(!report.worst_files.is_empty());
        assert!(!report.by_directory.is_empty());
    }

    #[test]
    fn test_score_without_duplication_dimension() {
        // Unmeasured duplication: the dimension disappears from the
        // report entirely instead of showing a misleading perfect grade.
        let result = make_test_result();
        let model = DefaultModel::without_duplication();
        let report = score(&result, &model, 10);
        assert!(report
            .dimensions
            .iter()
            .all(|d| d.dimension != HealthDimension::Duplication));
        assert!(report.worst_files.iter().all(|f| {
            f.dimensions
                .iter()
                .all(|d| d.dimension != HealthDimension::Duplication)
        }));
    }

    #[test]
    fn test_serialized_weights_are_normalized() {
        // The total score divides by the weight sum, so the serialized
        // per-dimension weights must be normalized the same way — a
        // five-dim report whose raw weights sum to 0.9 would misstate
        // every dimension's real contribution to the total.
        let result = make_test_result();

        let full = score(&result, &DefaultModel::new(), 10);
        let full_sum: f64 = full.dimensions.iter().map(|d| d.weight).sum();
        assert!((full_sum - 1.0).abs() < 1e-9);
        let complexity = full
            .dimensions
            .iter()
            .find(|d| d.dimension == HealthDimension::Complexity)
            .unwrap();
        assert!(
            (complexity.weight - 0.30).abs() < 1e-9,
            "six-dim weights already sum to 1 and must stay as declared"
        );

        let nodup = score(&result, &DefaultModel::without_duplication(), 10);
        let nodup_sum: f64 = nodup.dimensions.iter().map(|d| d.weight).sum();
        assert!(
            (nodup_sum - 1.0).abs() < 1e-9,
            "five-dim weights must renormalize to 1, got {nodup_sum}"
        );
        // File-level dimensions go through the same path.
        let file_sum: f64 = nodup.worst_files[0]
            .dimensions
            .iter()
            .map(|d| d.weight)
            .sum();
        assert!((file_sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_worst_files_sorted_ascending() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        for window in report.worst_files.windows(2) {
            assert!(window[0].score <= window[1].score);
        }
    }

    #[test]
    fn test_bad_file_has_lower_score() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        let bad = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("bad.rs"))
            .unwrap();
        let good = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("good.rs"))
            .unwrap();
        assert!(bad.score < good.score);
    }

    #[test]
    fn test_directory_grouping() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.by_directory.len(), 3);
        let dir_paths: Vec<&Path> = report
            .by_directory
            .iter()
            .map(|d| d.path.as_path())
            .collect();
        assert!(dir_paths.contains(&Path::new("src")));
        assert!(dir_paths.contains(&Path::new("lib")));
        assert!(dir_paths.contains(&Path::new(".")));
        let root = report
            .by_directory
            .iter()
            .find(|directory| directory.path == Path::new("."))
            .unwrap();
        assert_eq!(root.file_count, 3);
    }

    #[test]
    fn test_top_n_limits() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 1);
        assert_eq!(report.worst_files.len(), 1);
        assert_eq!(report.by_directory.len(), 1);
    }

    #[test]
    fn test_empty_result() {
        let result = AnalysisResult {
            files: vec![],
            summary: Summary::default(),
            elapsed: Duration::from_millis(1),
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        };
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        assert_eq!(report.score, 0.0);
        assert_eq!(report.grade, Grade::F);
        assert!(report.dimensions.is_empty());
        assert!(report.worst_files.is_empty());
        assert!(report.by_directory.is_empty());
    }

    fn worsen(file: &mut FileStats) {
        file.lines.total = 900;
        file.lines.code = 850;
        file.complexity.cyclomatic = 120;
        file.complexity.functions = 2;
        file.complexity.max_depth = 9;
        file.complexity.avg_func_lines = 400.0;
    }

    #[test]
    fn test_compare_no_change_passes() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let reg = compare_with_baseline(&result, &result, &model, "latest");
        assert!(!reg.failed);
        assert!(!reg.project_regressed);
        assert!(reg.regressed_files.is_empty());
        assert!(reg.score_delta.abs() < 0.001);
        assert_eq!(reg.model, "default-v2");
        assert!((reg.current_score - reg.baseline_score - reg.score_delta).abs() < 0.001);
        assert_eq!(reg.current_grade, reg.baseline_grade);
    }

    #[test]
    fn test_compare_detects_file_regression() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        worsen(&mut current.files[0]); // good.rs degrades
        current.summary = Summary::from_file_stats(&current.files);

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(reg.failed);
        assert_eq!(reg.regressed_files.len(), 1);
        let fr = &reg.regressed_files[0];
        assert!(fr.path.ends_with("good.rs"));
        assert!(fr.to_grade > fr.from_grade);
    }

    #[test]
    fn test_compare_added_f_file_regresses() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        let mut extra = current.files[1].clone();
        extra.path = PathBuf::from("src/new_horror.rs");
        worsen(&mut extra);
        current.files.push(extra);
        current.summary = Summary::from_file_stats(&current.files);

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(reg.failed);
        assert_eq!(reg.regressed_files.len(), 1);
        assert!(reg.regressed_files[0].is_new);
        assert!(reg.regressed_files[0].path.ends_with("new_horror.rs"));
    }

    #[test]
    fn test_compare_counts_improvements() {
        let mut baseline = make_test_result();
        worsen(&mut baseline.files[0]);
        let current = make_test_result();

        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(reg.improved_files >= 1);
        assert!(reg.regressed_files.is_empty());
    }

    #[test]
    fn test_compare_detects_five_point_drop_within_same_grade() {
        let baseline = result_with_file_score("src/a.rs", 85);
        let current = result_with_file_score("src/a.rs", 80);

        let reg = compare_with_baseline(&baseline, &current, &FileLineScoreModel, "main");

        assert!(reg.failed);
        assert!(reg.project_regressed);
        assert_eq!(reg.baseline_grade, Grade::B);
        assert_eq!(reg.current_grade, Grade::B);
        assert_eq!(reg.regressed_files.len(), 1);
        assert!(!reg.regressed_files[0].is_new);
    }

    #[test]
    fn test_compare_ignores_drop_below_five_points_in_same_grade() {
        let baseline = result_with_file_score("src/a.rs", 85);
        let current = result_with_file_score("src/a.rs", 81);

        let reg = compare_with_baseline(&baseline, &current, &FileLineScoreModel, "main");

        assert!(!reg.failed);
        assert!(!reg.project_regressed);
        assert!(reg.regressed_files.is_empty());
    }

    #[test]
    fn test_compare_uses_only_measurements_available_on_both_sides() {
        let mut baseline = make_test_result();
        for file in &mut baseline.files {
            file.complexity.functions_measured = false;
            file.complexity.control_flow_measured = false;
        }
        let mut current = baseline.clone();
        for file in &mut current.files {
            file.complexity.functions_measured = true;
            file.complexity.control_flow_measured = true;
            file.complexity.functions = 1;
            file.complexity.cyclomatic = 500;
            file.complexity.max_depth = 20;
            file.complexity.avg_func_lines = 500.0;
        }

        let reg = compare_with_baseline(&baseline, &current, &DefaultModel::new(), "old");

        assert!(reg.score_delta.abs() < 0.001, "{}", reg.score_delta);
        assert!(!reg.failed);
        assert!(reg.regressed_files.is_empty());
    }

    #[test]
    fn test_compare_marks_scope_change_without_project_regression() {
        let baseline = result_with_direct_scores(&[("tests/a.rs", "Rust", 95)]);
        let current = result_with_direct_scores(&[("src/a.rs", "Rust", 80)]);

        let reg = compare_with_baseline(&baseline, &current, &DirectScoreV2Model, "main");

        assert_eq!(reg.baseline_scope, HealthScope::TestsOnly);
        assert_eq!(reg.current_scope, HealthScope::Production);
        assert!(reg.scope_changed);
        assert!(!reg.project_regressed);
        assert!(!reg.failed);
    }

    #[test]
    fn test_compare_reports_language_health_changes() {
        let baseline =
            result_with_direct_scores(&[("src/a.rs", "Rust", 90), ("src/tool.py", "Python", 70)]);
        let current = result_with_direct_scores(&[
            ("src/a.rs", "Rust", 80),
            ("src/tool.ts", "TypeScript", 95),
        ]);

        let reg = compare_with_baseline(&baseline, &current, &DirectScoreV2Model, "main");

        let rust = reg
            .by_language
            .iter()
            .find(|language| language.language == "Rust")
            .unwrap();
        assert_eq!(rust.status, LanguageHealthChangeStatus::Changed);
        assert_eq!(rust.score_delta, Some(-10.0));

        let python = reg
            .by_language
            .iter()
            .find(|language| language.language == "Python")
            .unwrap();
        assert_eq!(python.status, LanguageHealthChangeStatus::Removed);

        let typescript = reg
            .by_language
            .iter()
            .find(|language| language.language == "TypeScript")
            .unwrap();
        assert_eq!(typescript.status, LanguageHealthChangeStatus::Added);
    }

    #[test]
    fn test_compare_allows_new_healthy_file() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        let mut extra = current.files[0].clone();
        extra.path = PathBuf::from("src/new_good.rs");
        current.files.push(extra);

        let reg = compare_with_baseline(&baseline, &current, &DefaultModel::new(), "main");

        assert!(reg.regressed_files.iter().all(|file| !file.is_new));
    }

    #[test]
    fn test_non_code_files_do_not_change_health() {
        let baseline = make_test_result();
        let baseline_report = score(&baseline, &DefaultModel::new(), 20);
        let mut with_document = baseline.clone();
        with_document.files.push(FileStats {
            path: PathBuf::from("docs/huge.md"),
            language: "Markdown".to_string(),
            lines: LineStats {
                total: 20_000,
                code: 19_000,
                comment: 0,
                blank: 1_000,
            },
            size: 500_000,
            duplicate_lines: 18_000,
            complexity: Complexity::default(),
        });

        let report = score(&with_document, &DefaultModel::new(), 20);

        assert!((report.score - baseline_report.score).abs() < 0.01);
        assert_eq!(report.worst_files.len(), baseline_report.worst_files.len());
        assert!(report
            .worst_files
            .iter()
            .all(|file| !file.path.ends_with("huge.md")));
    }

    #[test]
    fn test_only_non_code_files_score_zero() {
        let mut result = make_test_result();
        result.files.retain(|file| file.path.ends_with("good.rs"));
        result.files[0].language = "JSON".to_string();

        let report = score(&result, &DefaultModel::new(), 10);

        assert_eq!(report.score, 0.0);
        assert_eq!(report.grade, Grade::F);
        assert!(report.dimensions.is_empty());
    }

    #[test]
    fn test_single_bad_file_is_reported_without_hard_capping_project() {
        let sample = make_test_result();
        let good = sample.files[0].clone();
        let mut files = Vec::new();
        for index in 0..20 {
            let mut file = good.clone();
            file.path = PathBuf::from(format!("src/good-{index}.rs"));
            files.push(file);
        }
        let mut bad = good;
        bad.path = PathBuf::from("src/bad.rs");
        worsen(&mut bad);
        files.push(bad);
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            scanned_files: files.len(),
            files,
            elapsed: Duration::ZERO,
            skipped_files: 0,
            error_files: 0,
        };

        let report = score(&result, &DefaultModel::new(), 50);
        let worst = report.worst_files.first().unwrap();

        assert!(report.score > worst.score + 30.0);
        assert!((report.tail_risk.worst_score - worst.score).abs() < 1e-9);
        assert_eq!(report.tail_risk.failing_files, 1);
    }

    #[test]
    fn test_recursive_directory_aggregation() {
        let mut result = make_test_result();
        let mut nested = result.files[0].clone();
        nested.path = PathBuf::from("src/nested/deep.rs");
        result.files.push(nested);

        let report = score(&result, &DefaultModel::new(), 20);
        let src = report
            .by_directory
            .iter()
            .find(|directory| directory.path == Path::new("src"))
            .unwrap();
        let nested = report
            .by_directory
            .iter()
            .find(|directory| directory.path == Path::new("src/nested"))
            .unwrap();

        assert_eq!(src.file_count, 3);
        assert_eq!(nested.file_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_absolute_directory_aggregation_stops_at_common_parent() {
        let mut result = make_test_result();
        result.files[0].path = PathBuf::from("/workspace/project/src/good.rs");
        result.files[1].path = PathBuf::from("/workspace/project/src/bad.rs");
        result.files[2].path = PathBuf::from("/workspace/project/lib/utils.rs");

        let report = score(&result, &DefaultModel::new(), 20);
        let paths: Vec<&Path> = report
            .by_directory
            .iter()
            .map(|directory| directory.path.as_path())
            .collect();

        assert!(paths.contains(&Path::new("/workspace/project")));
        assert!(!paths.contains(&Path::new("/workspace")));
        assert!(!paths.contains(&Path::new("/")));
    }

    #[test]
    fn test_directory_aggregation_deduplicates_overlapping_inputs() {
        let mut result = make_test_result();
        result.files.push(result.files[0].clone());

        let report = score(&result, &DefaultModel::new(), 20);
        let src = report
            .by_directory
            .iter()
            .find(|directory| directory.path == Path::new("src"))
            .unwrap();

        assert_eq!(src.file_count, 2);
        assert_eq!(report.worst_files.len(), 3);
    }

    #[test]
    fn test_top_issue_uses_weighted_loss() {
        let result = result_with_file_score("src/a.rs", 100);
        let mut file = result.files[0].clone();
        file.lines.comment = 10;
        file.complexity = Complexity {
            functions_measured: true,
            control_flow_measured: true,
            functions: 1,
            cyclomatic: 15,
            cognitive: 0,
            max_depth: 12,
            avg_func_lines: 15.0,
            ..Complexity::default()
        };

        let health = score_file(&file, &DefaultModel::new());

        assert_eq!(health.top_issue, HealthDimension::Complexity);
        let nesting = health
            .dimensions
            .iter()
            .find(|dimension| dimension.dimension == HealthDimension::NestingDepth)
            .unwrap();
        let complexity = health
            .dimensions
            .iter()
            .find(|dimension| dimension.dimension == HealthDimension::Complexity)
            .unwrap();
        assert!(nesting.score < complexity.score);
    }

    #[test]
    fn test_compare_normalizes_dot_prefix() {
        let baseline = make_test_result();
        let mut current = make_test_result();
        for f in &mut current.files {
            f.path = PathBuf::from("./").join(&f.path);
        }
        let model = DefaultModel::new();
        let reg = compare_with_baseline(&baseline, &current, &model, "main");
        assert!(
            !reg.failed,
            "./-prefixed paths must match their baseline counterparts"
        );
    }

    #[test]
    fn test_file_top_issue() {
        let result = make_test_result();
        let model = DefaultModel::new();
        let report = score(&result, &model, 10);
        let bad = report
            .worst_files
            .iter()
            .find(|f| f.path.ends_with("bad.rs"))
            .unwrap();
        assert!(bad.dimensions.iter().any(|d| d.dimension == bad.top_issue));
    }
}
