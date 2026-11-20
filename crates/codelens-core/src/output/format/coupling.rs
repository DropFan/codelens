//! Formatter-neutral presentation data for coupling reports.

use crate::insight::coupling::CouplingReport;

#[derive(Debug, Clone)]
pub(crate) struct CouplingPresentation {
    pub period: String,
    pub total_commits: usize,
    pub focus: Option<String>,
    pub filter_notes: Vec<CouplingFilterPresentation>,
    pub rows: Vec<CouplingPairPresentation>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CouplingFilterPresentation {
    BulkCommits(usize),
    TestFiles(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct CouplingPairPresentation {
    pub file_a: String,
    pub file_b: String,
    pub shared: String,
    pub degree: f64,
    pub degree_text: String,
}

impl CouplingPresentation {
    pub(crate) fn from_report(report: &CouplingReport) -> Self {
        let mut filter_notes = Vec::with_capacity(2);
        if report.skipped_large_commits > 0 {
            filter_notes.push(CouplingFilterPresentation::BulkCommits(
                report.skipped_large_commits,
            ));
        }
        if report.excluded_test_files > 0 {
            filter_notes.push(CouplingFilterPresentation::TestFiles(
                report.excluded_test_files,
            ));
        }

        let rows = report
            .pairs
            .iter()
            .map(|pair| CouplingPairPresentation {
                file_a: pair.file_a.display().to_string(),
                file_b: pair.file_b.display().to_string(),
                shared: format!(
                    "{} ({}/{})",
                    pair.shared_commits, pair.commits_a, pair.commits_b
                ),
                degree: pair.degree,
                degree_text: format!("{:.0}%", pair.degree),
            })
            .collect();

        Self {
            period: report.since.clone(),
            total_commits: report.total_commits,
            focus: report.focus.as_ref().map(|path| path.display().to_string()),
            filter_notes,
            rows,
        }
    }
}
