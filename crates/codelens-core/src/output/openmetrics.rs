//! OpenMetrics (Prometheus text exposition) output format.
//!
//! Lets a Prometheus-compatible scraper collect codebase metrics over
//! time — a snapshot-free alternative to `trend` for long-term tracking.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions, Report};

/// OpenMetrics output formatter.
pub struct OpenMetricsOutput;

impl OpenMetricsOutput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OpenMetricsOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for OpenMetricsOutput {
    fn name(&self) -> &'static str {
        "openmetrics"
    }

    fn extension(&self) -> &'static str {
        "txt"
    }

    fn write(
        &self,
        report: &Report,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => write_analysis(result, writer)?,
            Report::Combined(combined) => {
                write_analysis(&combined.analysis, writer)?;
                write_health(&combined.health, writer)?;
            }
            Report::Health(report) => write_health(report, writer)?,
            Report::Hotspot(report) => {
                writeln!(writer, "# TYPE codelens_hotspot_files gauge")?;
                writeln!(writer, "codelens_hotspot_files {}", report.files.len())?;
                writeln!(writer, "# TYPE codelens_hotspot_commits gauge")?;
                writeln!(writer, "codelens_hotspot_commits {}", report.total_commits)?;
            }
            Report::Coupling(report) => {
                writeln!(writer, "# TYPE codelens_coupling_pairs gauge")?;
                writeln!(writer, "codelens_coupling_pairs {}", report.pairs.len())?;
                writeln!(writer, "# TYPE codelens_coupling_commits gauge")?;
                writeln!(writer, "codelens_coupling_commits {}", report.total_commits)?;
            }
            Report::Diff(report) => {
                writeln!(writer, "# TYPE codelens_diff_code_delta gauge")?;
                writeln!(
                    writer,
                    "codelens_diff_code_delta {}",
                    report.delta.code.signed_delta()
                )?;
                writeln!(writer, "# TYPE codelens_diff_health_delta gauge")?;
                writeln!(
                    writer,
                    "codelens_diff_health_delta{{model=\"{}\"}} {:.1}",
                    escape_label(&report.health.model),
                    report.health.score_delta
                )?;
                writeln!(writer, "# TYPE codelens_diff_regressed_files gauge")?;
                writeln!(
                    writer,
                    "codelens_diff_regressed_files{{model=\"{}\"}} {}",
                    escape_label(&report.health.model),
                    report.health.regressed_files.len()
                )?;
            }
            Report::Trend(report) => {
                writeln!(writer, "# TYPE codelens_trend_code_delta gauge")?;
                writeln!(
                    writer,
                    "codelens_trend_code_delta {}",
                    report.delta.code.signed_delta()
                )?;
            }
            Report::Estimation(report) => {
                writeln!(writer, "# TYPE codelens_estimated_cost_dollars gauge")?;
                writeln!(
                    writer,
                    "codelens_estimated_cost_dollars {}",
                    report.estimated_cost
                )?;
            }
            Report::EstimationComparison(report) => {
                writeln!(writer, "# TYPE codelens_estimated_cost_dollars gauge")?;
                for r in &report.reports {
                    writeln!(
                        writer,
                        "codelens_estimated_cost_dollars{{model=\"{}\"}} {}",
                        escape_label(&r.model),
                        r.estimated_cost
                    )?;
                }
            }
        }
        writeln!(writer, "# EOF")?;
        Ok(())
    }
}

fn write_analysis(result: &AnalysisResult, writer: &mut dyn Write) -> Result<()> {
    let s = &result.summary;
    writeln!(writer, "# TYPE codelens_files gauge")?;
    writeln!(writer, "codelens_files {}", s.total_files)?;

    writeln!(writer, "# TYPE codelens_lines gauge")?;
    for (kind, value) in [
        ("code", s.lines.code),
        ("comment", s.lines.comment),
        ("blank", s.lines.blank),
    ] {
        writeln!(writer, "codelens_lines{{kind=\"{kind}\"}} {value}")?;
    }

    writeln!(writer, "# TYPE codelens_language_code_lines gauge")?;
    for (name, stats) in &s.by_language {
        writeln!(
            writer,
            "codelens_language_code_lines{{language=\"{}\"}} {}",
            escape_label(name),
            stats.lines.code
        )?;
    }
    Ok(())
}

fn write_health(
    report: &crate::insight::health::HealthReport,
    writer: &mut dyn Write,
) -> Result<()> {
    writeln!(writer, "# TYPE codelens_health_score gauge")?;
    writeln!(
        writer,
        "codelens_health_score{{model=\"{}\"}} {}",
        escape_label(&report.model),
        report.score
    )?;
    writeln!(writer, "# TYPE codelens_health_confidence_ratio gauge")?;
    writeln!(
        writer,
        "codelens_health_confidence_ratio{{model=\"{}\",scope=\"{}\"}} {}",
        escape_label(&report.model),
        report.scope,
        report.confidence.coverage
    )?;
    writeln!(writer, "# TYPE codelens_health_tail_score gauge")?;
    writeln!(
        writer,
        "codelens_health_tail_score{{model=\"{}\"}} {}",
        escape_label(&report.model),
        report.tail_risk.tail_score
    )?;
    writeln!(writer, "# TYPE codelens_health_failing_files gauge")?;
    writeln!(
        writer,
        "codelens_health_failing_files{{model=\"{}\"}} {}",
        escape_label(&report.model),
        report.tail_risk.failing_files
    )?;
    writeln!(writer, "# TYPE codelens_health_dimension_score gauge")?;
    for dim in &report.dimensions {
        writeln!(
            writer,
            "codelens_health_dimension_score{{model=\"{}\",dimension=\"{}\"}} {}",
            escape_label(&report.model),
            escape_label(&dim.dimension.to_string()),
            dim.score
        )?;
    }
    writeln!(writer, "# TYPE codelens_language_health_score gauge")?;
    writeln!(
        writer,
        "# TYPE codelens_language_health_confidence_ratio gauge"
    )?;
    writeln!(writer, "# TYPE codelens_language_health_files gauge")?;
    for language in &report.by_language {
        let name = escape_label(&language.language);
        let model = escape_label(&report.model);
        writeln!(
            writer,
            "codelens_language_health_score{{model=\"{model}\",language=\"{name}\"}} {}",
            language.score
        )?;
        writeln!(
            writer,
            "codelens_language_health_confidence_ratio{{model=\"{model}\",language=\"{name}\"}} {}",
            language.confidence.coverage
        )?;
        writeln!(
            writer,
            "codelens_language_health_files{{model=\"{model}\",language=\"{name}\"}} {}",
            language.file_count
        )?;
    }
    if let Some(test_health) = &report.test_health {
        writeln!(writer, "# TYPE codelens_test_health_score gauge")?;
        writeln!(
            writer,
            "codelens_test_health_score{{model=\"{}\"}} {}",
            escape_label(&report.model),
            test_health.score
        )?;
    }
    Ok(())
}

/// Escape a label value per the OpenMetrics spec.
fn escape_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{FileStats, LineStats, Summary};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn test_openmetrics_analysis() {
        let files = vec![FileStats {
            path: PathBuf::from("a.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 10,
                code: 8,
                comment: 1,
                blank: 1,
            },
            size: 100,
            duplicate_lines: 0,
            complexity: Default::default(),
        }];
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(1),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };

        let mut buf = Vec::new();
        OpenMetricsOutput::new()
            .write(
                &Report::Analysis(result),
                &OutputOptions::default(),
                &mut buf,
            )
            .unwrap();
        let text = String::from_utf8(buf).unwrap();

        assert!(text.contains("codelens_lines{kind=\"code\"} 8"));
        assert!(text.contains("codelens_language_code_lines{language=\"Rust\"} 8"));
        assert!(text.ends_with("# EOF\n"));
    }

    #[test]
    fn test_openmetrics_health_includes_model_label() {
        let report = crate::insight::health::HealthReport {
            score: 82.5,
            grade: crate::insight::Grade::B,
            model: "default-v2-no-dup".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![],
            scope: crate::insight::health::HealthScope::Production,
            confidence: Default::default(),
            tail_risk: Default::default(),
            by_language: vec![],
            test_health: None,
            regression: None,
        };
        let mut buf = Vec::new();

        OpenMetricsOutput::new()
            .write(&Report::Health(report), &OutputOptions::default(), &mut buf)
            .unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("codelens_health_score{model=\"default-v2-no-dup\"} 82.5"));
    }

    #[test]
    fn test_openmetrics_diff_health_metrics_include_model_label() {
        let files = vec![FileStats {
            path: PathBuf::from("a.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 10,
                code: 8,
                comment: 1,
                blank: 1,
            },
            size: 100,
            duplicate_lines: 0,
            complexity: Default::default(),
        }];
        let result = AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(1),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };
        let model = crate::insight::scoring::default::DefaultModel::without_duplication();
        let report = crate::insight::diff::build("before", "after", &result, &result, &model);
        let mut buf = Vec::new();

        OpenMetricsOutput::new()
            .write(
                &Report::Diff(Box::new(report)),
                &OutputOptions::default(),
                &mut buf,
            )
            .unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("codelens_diff_health_delta{model=\"default-v2-no-dup\"} 0.0"));
        assert!(text.contains("codelens_diff_regressed_files{model=\"default-v2-no-dup\"} 0"));
    }
}
