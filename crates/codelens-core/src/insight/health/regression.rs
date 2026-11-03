//! Baseline comparison and regression-gate reporting.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::Serialize;

use crate::analyzer::stats::AnalysisResult;
use crate::insight::scoring::{HealthScoringFlow, ScoringModel};
use crate::insight::Grade;

use super::{normalized_path, score, HealthAssessment, HealthScope, LanguageHealth};

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
    /// Whether both sides provide enough measurements for a gateable delta.
    pub assessment: HealthAssessment,
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
    /// Whether the comparison found an actual project or file regression.
    /// Use [`Self::gate_failed`] when enforcing a quality gate so an
    /// insufficient-data comparison cannot pass on an estimated delta.
    pub failed: bool,
}

impl RegressionReport {
    /// Whether this comparison can satisfy `--fail-on-regression`.
    pub fn gate_failed(&self) -> bool {
        self.assessment == HealthAssessment::InsufficientData || self.failed
    }
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
    let assessment = if baseline_report.assessment == HealthAssessment::Scored
        && current_report.assessment == HealthAssessment::Scored
    {
        HealthAssessment::Scored
    } else {
        HealthAssessment::InsufficientData
    };
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
        assessment,
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
pub(super) fn comparable_results(
    baseline: &AnalysisResult,
    current: &AnalysisResult,
    model: &dyn ScoringModel,
) -> (AnalysisResult, AnalysisResult) {
    let mut baseline = baseline.clone();
    let mut current = current.clone();
    if model.health_scoring_flow() != HealthScoringFlow::FileFirstV2 {
        return (baseline, current);
    }

    // Region-aware and legacy snapshots must not be compared using different
    // production/test boundaries. Fall back to full-file data on both sides
    // unless both analyses explicitly support the projection.
    if !(baseline.summary.inline_test_regions_scanned
        && current.summary.inline_test_regions_scanned)
    {
        baseline.summary.inline_test_regions_scanned = false;
        current.summary.inline_test_regions_scanned = false;
        baseline.summary.health_regions.clear();
        current.summary.health_regions.clear();
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
        for regions in baseline
            .summary
            .health_regions
            .values_mut()
            .chain(current.summary.health_regions.values_mut())
        {
            for region in [&mut regions.production, &mut regions.tests] {
                if !functions_available {
                    region.complexity.functions_measured = false;
                }
                if !control_flow_available {
                    region.complexity.control_flow_measured = false;
                }
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
