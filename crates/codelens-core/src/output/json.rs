//! JSON output format.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions};

/// JSON output formatter.
pub struct JsonOutput {
    pretty: bool,
}

impl JsonOutput {
    /// Create a new JSON output formatter.
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl OutputFormat for JsonOutput {
    fn name(&self) -> &'static str {
        "json"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn write(
        &self,
        result: &AnalysisResult,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if self.pretty {
            serde_json::to_writer_pretty(&mut *writer, result)?;
        } else {
            serde_json::to_writer(&mut *writer, result)?;
        }
        writeln!(writer)?;
        Ok(())
    }
}
