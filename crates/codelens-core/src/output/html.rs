//! HTML output format with interactive charts.

use std::io::Write;

use askama::Template;

use crate::analyzer::stats::{AnalysisResult, LanguageSummary, Summary};
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions, Report};

/// HTML report template.
#[derive(Template)]
#[template(path = "report.html")]
struct HtmlReport<'a> {
    title: &'a str,
    generated_at: String,
    summary: &'a Summary,
    by_language: Vec<(&'a str, &'a LanguageSummary)>,
    elapsed_secs: f64,
}

/// HTML output formatter.
pub struct HtmlOutput;

impl HtmlOutput {
    /// Create a new HTML output formatter.
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
            Report::Health(report) => self.write_json_html("Code Health Report", report, writer),
            Report::Hotspot(report) => {
                self.write_json_html("Hotspot Analysis", report, writer)
            }
            Report::Trend(report) => self.write_json_html("Trend Report", report, writer),
        }
    }
}

impl HtmlOutput {
    fn write_json_html<T: serde::Serialize>(
        &self,
        title: &str,
        data: &T,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        writeln!(writer, "<!DOCTYPE html>")?;
        writeln!(writer, "<html><head><meta charset=\"utf-8\">")?;
        writeln!(writer, "<title>Codelens - {title}</title>")?;
        writeln!(
            writer,
            "<style>body{{font-family:monospace;margin:2em;}}pre{{background:#f5f5f5;padding:1em;overflow:auto;}}</style>"
        )?;
        writeln!(writer, "</head><body>")?;
        writeln!(writer, "<h1>{title}</h1>")?;
        writeln!(writer, "<pre>{json}</pre>")?;
        writeln!(writer, "</body></html>")?;
        Ok(())
    }

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

        let report = HtmlReport {
            title: "Codelens - Code Statistics Report",
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            summary: &result.summary,
            by_language,
            elapsed_secs: result.elapsed.as_secs_f64(),
        };

        write!(writer, "{}", report.render()?)?;
        Ok(())
    }
}
