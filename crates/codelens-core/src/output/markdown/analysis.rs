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
        let view = AnalysisPresentation::from_result(result, options.top_n);

        writeln!(writer, "# Code Statistics Report")?;
        writeln!(writer)?;

        // Summary
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

        if options.show_tokens && view.tokens_est > 0 {
            for window in crate::analyzer::tokens::CONTEXT_WINDOWS {
                let ratio = view.tokens_est as f64 / window.tokens as f64;
                if ratio <= 1.0 {
                    writeln!(
                        writer,
                        "- {}: fits ({:.0}% used)",
                        window.label,
                        ratio * 100.0
                    )?;
                } else {
                    writeln!(writer, "- {}: {:.1}x over", window.label, ratio)?;
                }
            }
            writeln!(
                writer,
                "\n_Token counts are byte-based estimates, not tokenizer-exact._"
            )?;
            writeln!(writer)?;
        }

        // Language breakdown
        if !options.summary_only && !view.languages.is_empty() {
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
        }

        // Per-directory breakdown (--by-dir)
        if options.by_dir && !result.files.is_empty() {
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
            for d in crate::analyzer::stats::aggregate_by_dir(&result.files, options.dir_depth) {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    d.path.display(),
                    d.files,
                    d.lines.code,
                    d.lines.comment,
                    d.lines.blank,
                    d.lines.total,
                    d.cyclomatic,
                )?;
            }
            writeln!(writer)?;
        }

        // Per-file breakdown (--by-file)
        if options.by_file && !result.files.is_empty() {
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
            for f in
                crate::output::format::sorted_files(&result.files, options.sort_by, options.top_n)
            {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} |",
                    f.path.display(),
                    f.language,
                    f.lines.code,
                    f.lines.comment,
                    f.lines.blank,
                    f.lines.total
                )?;
            }
            writeln!(writer)?;
        }

        writeln!(writer, "---")?;
        writeln!(
            writer,
            "*Generated by codelens in {:.2}s*",
            view.elapsed_secs
        )?;

        Ok(())
    }
}
