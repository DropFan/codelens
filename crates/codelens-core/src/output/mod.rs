//! Output format implementations.

mod badge;
mod console;
mod csv;
mod format;
mod html;
mod json;
mod markdown;
mod openmetrics;
mod sarif;

pub use badge::BadgeOutput;
pub use console::ConsoleOutput;
pub use csv::CsvOutput;
pub use format::{CombinedReport, OutputFormat, OutputOptions, Report};
pub use html::HtmlOutput;
pub use json::JsonOutput;
pub use markdown::MarkdownOutput;
pub use openmetrics::OpenMetricsOutput;
pub use sarif::SarifOutput;

use crate::config::OutputFormatType;

/// Create an output formatter for the given type.
pub fn create_output(format: OutputFormatType) -> Box<dyn OutputFormat> {
    match format {
        OutputFormatType::Console => Box::new(ConsoleOutput::new()),
        OutputFormatType::Json => Box::new(JsonOutput::new(true)),
        OutputFormatType::Csv => Box::new(CsvOutput::new()),
        OutputFormatType::Markdown => Box::new(MarkdownOutput::new()),
        OutputFormatType::Html => Box::new(HtmlOutput::new()),
        OutputFormatType::OpenMetrics => Box::new(OpenMetricsOutput::new()),
        OutputFormatType::Badge => Box::new(BadgeOutput::new()),
        OutputFormatType::Sarif => Box::new(SarifOutput::new()),
    }
}
