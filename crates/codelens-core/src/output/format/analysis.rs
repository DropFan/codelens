//! Formatter-neutral statistics shared by human-readable analysis outputs.

use crate::analyzer::stats::AnalysisResult;

use super::{sorted_files, OutputOptions};

/// Formatter-neutral statistics used by human-readable analysis reports.
///
/// This view owns field selection and derived values while console, Markdown,
/// and HTML remain responsible for their own layout and number decoration.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisPresentation {
    pub total_files: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
    pub total_size: u64,
    pub language_count: usize,
    pub functions: usize,
    pub complexity: Option<AnalysisComplexityPresentation>,
    pub tests: Option<AnalysisTestPresentation>,
    pub duplication: Option<AnalysisDuplicationPresentation>,
    pub tokens_est: u64,
    pub token_windows: Vec<AnalysisTokenWindowPresentation>,
    pub languages: Vec<AnalysisLanguagePresentation>,
    pub show_languages: bool,
    pub directories: Vec<AnalysisDirectoryPresentation>,
    pub files: Vec<AnalysisFilePresentation>,
    pub elapsed_secs: f64,
    pub scanned_files: usize,
    pub skipped_files: usize,
    pub error_files: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisTokenWindowPresentation {
    pub label: &'static str,
    pub fit_percent: Option<f64>,
    pub over_ratio: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisComplexityPresentation {
    pub cyclomatic: usize,
    pub cognitive: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisTestPresentation {
    pub files: usize,
    pub code_lines: usize,
    pub code_ratio: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisDuplicationPresentation {
    pub unique_lines: usize,
    pub dryness_percent: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisLanguagePresentation {
    pub name: String,
    pub files: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
    pub tokens_est: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisDirectoryPresentation {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub files: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
    pub cyclomatic: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisFilePresentation {
    pub path: String,
    pub language: String,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
}

impl AnalysisPresentation {
    pub(crate) fn from_result(result: &AnalysisResult, options: &OutputOptions) -> Self {
        let summary = &result.summary;
        let production_code = summary.lines.code.saturating_sub(summary.test_lines.code);
        let tests = (summary.test_files > 0).then(|| AnalysisTestPresentation {
            files: summary.test_files,
            code_lines: summary.test_lines.code,
            code_ratio: (production_code > 0)
                .then_some(summary.test_lines.code as f64 / production_code as f64),
        });
        let duplication = (summary.uloc > 0).then(|| {
            let non_blank = summary.lines.total.saturating_sub(summary.lines.blank);
            AnalysisDuplicationPresentation {
                unique_lines: summary.uloc,
                dryness_percent: if non_blank > 0 {
                    summary.uloc as f64 / non_blank as f64 * 100.0
                } else {
                    100.0
                },
            }
        });
        let mut languages: Vec<_> = summary
            .by_language
            .iter()
            .map(|(name, stats)| AnalysisLanguagePresentation {
                name: name.clone(),
                files: stats.files,
                code_lines: stats.lines.code,
                comment_lines: stats.lines.comment,
                blank_lines: stats.lines.blank,
                total_lines: stats.lines.total,
                tokens_est: stats.tokens_est,
            })
            .collect();
        if let Some(limit) = options.top_n {
            languages.truncate(limit);
        }
        let show_languages = !options.summary_only && !languages.is_empty();
        let token_windows = analysis_token_windows(summary.tokens_est, options.show_tokens);
        let directories = analysis_directories(result, options);
        let files = analysis_files(result, options);

        Self {
            total_files: summary.total_files,
            code_lines: summary.lines.code,
            comment_lines: summary.lines.comment,
            blank_lines: summary.lines.blank,
            total_lines: summary.lines.total,
            total_size: summary.total_size,
            language_count: summary.by_language.len(),
            functions: summary.complexity.functions,
            complexity: (summary.complexity.cyclomatic > 0).then_some({
                AnalysisComplexityPresentation {
                    cyclomatic: summary.complexity.cyclomatic,
                    cognitive: summary.complexity.cognitive,
                }
            }),
            tests,
            duplication,
            tokens_est: summary.tokens_est,
            token_windows,
            languages,
            show_languages,
            directories,
            files,
            elapsed_secs: result.elapsed.as_secs_f64(),
            scanned_files: result.scanned_files,
            skipped_files: result.skipped_files,
            error_files: result.error_files,
        }
    }
}

fn analysis_token_windows(
    tokens_est: u64,
    show_tokens: bool,
) -> Vec<AnalysisTokenWindowPresentation> {
    if !show_tokens || tokens_est == 0 {
        return Vec::new();
    }
    crate::analyzer::tokens::CONTEXT_WINDOWS
        .iter()
        .map(|window| {
            let ratio = tokens_est as f64 / window.tokens as f64;
            AnalysisTokenWindowPresentation {
                label: window.label,
                fit_percent: (ratio <= 1.0).then_some(ratio * 100.0),
                over_ratio: (ratio > 1.0).then_some(ratio),
            }
        })
        .collect()
}

fn analysis_directories(
    result: &AnalysisResult,
    options: &OutputOptions,
) -> Vec<AnalysisDirectoryPresentation> {
    if !options.by_dir || result.files.is_empty() {
        return Vec::new();
    }
    crate::analyzer::stats::aggregate_by_dir(&result.files, options.dir_depth)
        .into_iter()
        .map(|directory| AnalysisDirectoryPresentation {
            path: directory.path.display().to_string(),
            name: directory
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| directory.path.display().to_string()),
            depth: directory.depth,
            files: directory.files,
            code_lines: directory.lines.code,
            comment_lines: directory.lines.comment,
            blank_lines: directory.lines.blank,
            total_lines: directory.lines.total,
            cyclomatic: directory.cyclomatic,
        })
        .collect()
}

fn analysis_files(
    result: &AnalysisResult,
    options: &OutputOptions,
) -> Vec<AnalysisFilePresentation> {
    if !options.by_file || result.files.is_empty() {
        return Vec::new();
    }
    sorted_files(&result.files, options.sort_by, options.top_n)
        .into_iter()
        .map(|file| AnalysisFilePresentation {
            path: file.path.display().to_string(),
            language: file.language.clone(),
            code_lines: file.lines.code,
            comment_lines: file.lines.comment,
            blank_lines: file.lines.blank,
            total_lines: file.lines.total,
        })
        .collect()
}
