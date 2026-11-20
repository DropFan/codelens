//! Markdown rendering for analysis reports.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;
use crate::output::format::{AnalysisPresentation, OutputOptions};

use super::MarkdownOutput;

impl MarkdownOutput {
    pub(super) fn write_analysis(
        &self,
        result: &AnalysisResult,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let view = AnalysisPresentation::from_result(result, options);
        writeln!(writer, "# Code Statistics Report")?;
        writeln!(writer)?;
        self.write_analysis_summary(&view, options, writer)?;
        self.write_analysis_token_windows(&view, writer)?;
        self.write_analysis_languages(&view, writer)?;
        self.write_analysis_directories(&view, writer)?;
        self.write_analysis_files(&view, writer)?;
        self.write_analysis_footer(&view, writer)
    }

    fn write_analysis_summary(
        &self,
        view: &AnalysisPresentation,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "## Summary")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Value |")?;
        writeln!(writer, "|--------|-------|")?;
        writeln!(writer, "| Total Files | {} |", view.total_files)?;
        writeln!(writer, "| Code Lines | {} |", view.code_lines)?;
        writeln!(writer, "| Comment Lines | {} |", view.comment_lines)?;
        writeln!(writer, "| Blank Lines | {} |", view.blank_lines)?;
        writeln!(writer, "| Total Lines | {} |", view.total_lines)?;
        writeln!(writer, "| Languages | {} |", view.language_count)?;
        if let Some(tests) = &view.tests {
            writeln!(writer, "| Test Files | {} |", tests.files)?;
            writeln!(writer, "| Test Code Lines | {} |", tests.code_lines)?;
            if let Some(ratio) = tests.code_ratio {
                writeln!(writer, "| Test/Code Ratio | {ratio:.2} |")?;
            }
        }
        if options.show_tokens {
            writeln!(
                writer,
                "| LLM Tokens (est.) | {} |",
                crate::analyzer::tokens::format_tokens(view.tokens_est)
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_token_windows(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.token_windows.is_empty() {
            return Ok(());
        }
        for window in &view.token_windows {
            match (window.fit_percent, window.over_ratio) {
                (Some(percent), _) => {
                    writeln!(writer, "- {}: fits ({percent:.0}% used)", window.label)?
                }
                (_, Some(ratio)) => writeln!(writer, "- {}: {ratio:.1}x over", window.label)?,
                _ => unreachable!("token window presentation always has a verdict"),
            }
        }
        writeln!(
            writer,
            "\n_Token counts are byte-based estimates, not tokenizer-exact._"
        )?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_languages(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !view.show_languages {
            return Ok(());
        }
        writeln!(writer, "## By Language")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "| Language | Files | Code | Comment | Blank | Total |"
        )?;
        writeln!(
            writer,
            "|----------|-------|------|---------|-------|-------|"
        )?;
        for language in &view.languages {
            writeln!(
                writer,
                "| {} | {} | {} | {} | {} | {} |",
                language.name,
                language.files,
                language.code_lines,
                language.comment_lines,
                language.blank_lines,
                language.total_lines
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_directories(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.directories.is_empty() {
            return Ok(());
        }
        writeln!(writer, "## By Directory")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "| Directory | Files | Code | Comment | Blank | Total | CC |"
        )?;
        writeln!(
            writer,
            "|-----------|-------|------|---------|-------|-------|-----|"
        )?;
        for directory in &view.directories {
            writeln!(
                writer,
                "| {} | {} | {} | {} | {} | {} | {} |",
                directory.path,
                directory.files,
                directory.code_lines,
                directory.comment_lines,
                directory.blank_lines,
                directory.total_lines,
                directory.cyclomatic,
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_files(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.files.is_empty() {
            return Ok(());
        }
        writeln!(writer, "## By File")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "| File | Language | Code | Comment | Blank | Total |"
        )?;
        writeln!(
            writer,
            "|------|----------|------|---------|-------|-------|"
        )?;
        for file in &view.files {
            writeln!(
                writer,
                "| {} | {} | {} | {} | {} | {} |",
                file.path,
                file.language,
                file.code_lines,
                file.comment_lines,
                file.blank_lines,
                file.total_lines
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_footer(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "---")?;
        writeln!(
            writer,
            "*Generated by codelens in {:.2}s*",
            view.elapsed_secs
        )?;
        Ok(())
    }
}
