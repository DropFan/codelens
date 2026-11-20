//! Console output with colors and formatting.

mod analysis;
mod health;

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::error::Result;
use crate::insight::health::HealthAssessment;

use super::format::{
    format_optional_delta, format_optional_health, OutputFormat, OutputOptions, Report,
};

/// Console output formatter.
pub struct ConsoleOutput;

impl ConsoleOutput {
    /// Create a new console output formatter.
    pub fn new() -> Self {
        Self
    }

    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn format_number(n: usize) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }
}

impl Default for ConsoleOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for ConsoleOutput {
    fn name(&self) -> &'static str {
        "console"
    }

    fn extension(&self) -> &'static str {
        "txt"
    }

    fn write(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if options.colorize {
            return self.write_report(report, options, writer);
        }
        // colored/comfy_table decide colors from the process tty, not the
        // actual writer, so strip escape codes when colors are unwanted
        // (e.g. writing to a file via -O).
        let mut buf = Vec::new();
        self.write_report(report, options, &mut buf)?;
        writer.write_all(&strip_ansi(&buf))?;
        Ok(())
    }
}

/// Remove ANSI CSI escape sequences (`ESC [ ... <final byte>`).
fn strip_ansi(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b && input.get(i + 1) == Some(&b'[') {
            i += 2;
            // Skip parameter/intermediate bytes until the final byte (0x40-0x7e)
            while i < input.len() && !(0x40..=0x7e).contains(&input[i]) {
                i += 1;
            }
            i += 1; // skip the final byte itself
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

impl ConsoleOutput {
    fn write_report(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(report) => self.write_health(report, options, writer),
            Report::Hotspot(report) => self.write_hotspot(report, options, writer),
            Report::Coupling(report) => self.write_coupling(report, options, writer),
            Report::Trend(report) => self.write_trend(report, options, writer),
            Report::Diff(report) => self.write_diff(report, options, writer),
            Report::Estimation(report) => self.write_estimation(report, options, writer),
            Report::EstimationComparison(report) => {
                self.write_estimation_comparison(report, writer)
            }
            Report::Combined(combined) => {
                self.write_analysis(&combined.analysis, options, writer)?;
                self.write_health(&combined.health, options, writer)?;
                self.write_estimation_comparison(&combined.estimation, writer)
            }
        }
    }
}

impl ConsoleOutput {
    fn write_hotspot(
        &self,
        report: &crate::insight::hotspot::HotspotReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        use crate::insight::hotspot::RiskLevel;

        // Header
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Hotspot Analysis ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        writeln!(
            writer,
            "  Period: {}  Total Commits: {}",
            report.since.bold(),
            Self::format_number(report.total_commits).bold()
        )?;
        writeln!(writer)?;

        if report.files.is_empty() {
            writeln!(writer, "  No hotspots found.")?;
            return Ok(());
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("File").add_attribute(Attribute::Bold),
            Cell::new("Chg").add_attribute(Attribute::Bold),
            Cell::new("+/-").add_attribute(Attribute::Bold),
            Cell::new("CC").add_attribute(Attribute::Bold),
            Cell::new("Age").add_attribute(Attribute::Bold),
            Cell::new("Auth").add_attribute(Attribute::Bold),
            Cell::new("Score").add_attribute(Attribute::Bold),
            Cell::new("Risk").add_attribute(Attribute::Bold),
        ]);

        for file in &report.files {
            let risk_color = match file.risk {
                RiskLevel::High => Color::Red,
                RiskLevel::Medium => Color::Yellow,
                RiskLevel::Low => Color::Green,
            };
            let age = file
                .age_days
                .map(crate::insight::hotspot::format_age)
                .unwrap_or_else(|| "-".to_string());
            let auth = match &file.knowledge {
                Some(k) if k.knowledge_island => format!("{} ★", k.authors),
                Some(k) => k.authors.to_string(),
                None => "-".to_string(),
            };
            table.add_row(vec![
                Cell::new(file.path.display().to_string()).fg(Color::Cyan),
                Cell::new(Self::format_number(file.churn.commits)),
                Cell::new(format!(
                    "+{}/-{}",
                    file.churn.lines_added, file.churn.lines_deleted
                )),
                Cell::new(file.complexity.cyclomatic.to_string()),
                Cell::new(age),
                Cell::new(auth),
                Cell::new(format!("{:.2}", file.hotspot_score)),
                Cell::new(file.risk.to_string()).fg(risk_color),
            ]);
        }

        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        // Knowledge islands: risky files that effectively one person knows.
        let islands: Vec<_> = report
            .files
            .iter()
            .filter_map(|f| {
                f.knowledge
                    .as_ref()
                    .filter(|k| k.knowledge_island)
                    .map(|k| (f, k))
            })
            .collect();
        if !islands.is_empty() {
            writeln!(
                writer,
                "{} {}",
                "★ Knowledge Islands".bold().red(),
                "(risky files effectively one person knows)".dimmed()
            )?;
            for (file, k) in islands {
                writeln!(
                    writer,
                    "    {}  {:.0}% by {}",
                    file.path.display().to_string().cyan(),
                    k.ownership * 100.0,
                    k.main_author
                )?;
            }
            writeln!(writer)?;
        }

        // Function-level breakdown (--functions)
        if report.files.iter().any(|f| f.functions.is_some()) {
            writeln!(
                writer,
                "{} {}",
                "Function Hotspots".bold(),
                "(approximate spans)".dimmed()
            )?;
            writeln!(writer)?;
            for file in report.files.iter().filter(|f| f.functions.is_some()) {
                writeln!(
                    writer,
                    "  {}",
                    file.path.display().to_string().cyan().bold()
                )?;
                for func in file.functions.as_deref().unwrap_or_default() {
                    writeln!(
                        writer,
                        "    {:<32} L{:<9} {} commits  CC {}",
                        func.name,
                        format!("{}-{}", func.start_line, func.end_line),
                        func.commits,
                        func.cyclomatic,
                    )?;
                }
            }
            writeln!(writer)?;
        }

        Ok(())
    }

    fn write_coupling(
        &self,
        report: &crate::insight::coupling::CouplingReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Change Coupling ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        writeln!(
            writer,
            "  Period: {}  Total Commits: {}",
            report.since.bold(),
            Self::format_number(report.total_commits).bold()
        )?;
        if let Some(focus) = &report.focus {
            writeln!(writer, "  Focus: {}", focus.display().to_string().bold())?;
        }
        if report.skipped_large_commits > 0 {
            writeln!(
                writer,
                "  {} bulk commit(s) excluded from pairing",
                report.skipped_large_commits
            )?;
        }
        if report.excluded_test_files > 0 {
            writeln!(
                writer,
                "  {} test file(s) excluded (--include-tests to include)",
                report.excluded_test_files
            )?;
        }
        writeln!(writer)?;

        if report.pairs.is_empty() {
            writeln!(writer, "  No coupled file pairs found.")?;
            writeln!(
                writer,
                "  (thresholds: --min-shared / --min-coupling can be lowered)"
            )?;
            return Ok(());
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("File A").add_attribute(Attribute::Bold),
            Cell::new("File B").add_attribute(Attribute::Bold),
            Cell::new("Shared").add_attribute(Attribute::Bold),
            Cell::new("Coupling").add_attribute(Attribute::Bold),
        ]);

        for pair in &report.pairs {
            let color = if pair.degree >= 70.0 {
                Color::Red
            } else if pair.degree >= 50.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            table.add_row(vec![
                Cell::new(pair.file_a.display().to_string()).fg(Color::Cyan),
                Cell::new(pair.file_b.display().to_string()).fg(Color::Cyan),
                Cell::new(format!(
                    "{} ({}/{})",
                    pair.shared_commits, pair.commits_a, pair.commits_b
                )),
                Cell::new(format!("{:.0}%", pair.degree)).fg(color),
            ]);
        }

        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "  {} Files changing together share a dependency the module\n  structure does not express — review the strongest pairs first.",
            "hint:".dimmed()
        )?;
        writeln!(writer)?;

        Ok(())
    }

    fn write_diff(
        &self,
        report: &crate::insight::diff::DiffReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Diff Report ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(writer, "  {} → {}", report.from.bold(), report.to.bold())?;

        let reg = &report.health;
        let delta_str = if reg.score_delta >= 0.0 {
            format!("+{:.1}", reg.score_delta).green().to_string()
        } else {
            format!("{:.1}", reg.score_delta).red().to_string()
        };
        writeln!(
            writer,
            "  Health [{}]: {} ({:.1}) → {} ({:.1})  Δ {}",
            reg.model,
            reg.baseline_grade,
            reg.baseline_score,
            reg.current_grade,
            reg.current_score,
            delta_str
        )?;
        if reg.scope_changed {
            writeln!(
                writer,
                "  Scope changed: {} → {} (project delta is informational)",
                reg.baseline_scope, reg.current_scope
            )?;
        }
        if reg.assessment == HealthAssessment::InsufficientData {
            writeln!(
                writer,
                "  Assessment: insufficient data (score delta is an estimate)"
            )?;
        }
        if reg.improved_files > 0 {
            writeln!(writer, "  Improved files: {}", reg.improved_files)?;
        }
        writeln!(writer)?;

        if !reg.regressed_files.is_empty() {
            writeln!(writer, "{}", "  Regressed Files".bold())?;
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("File").add_attribute(Attribute::Bold),
                Cell::new("Before").add_attribute(Attribute::Bold),
                Cell::new("After").add_attribute(Attribute::Bold),
            ]);
            for f in &reg.regressed_files {
                let before = if f.is_new {
                    "NEW".to_string()
                } else {
                    format!("{} ({:.1})", f.from_grade, f.from_score)
                };
                table.add_row(vec![
                    Cell::new(f.path.display().to_string()).fg(Color::Cyan),
                    Cell::new(before).fg(if f.is_new {
                        Color::Cyan
                    } else {
                        Self::grade_color(f.from_grade)
                    }),
                    Cell::new(format!("{} ({:.1})", f.to_grade, f.to_score))
                        .fg(Self::grade_color(f.to_grade)),
                ]);
            }
            writeln!(writer, "{table}")?;
            writeln!(writer)?;
        }

        if !reg.by_language.is_empty() {
            writeln!(writer, "{}", "  Language Health Changes".bold())?;
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Status").add_attribute(Attribute::Bold),
                Cell::new("Before").add_attribute(Attribute::Bold),
                Cell::new("After").add_attribute(Attribute::Bold),
                Cell::new("Delta").add_attribute(Attribute::Bold),
            ]);
            for language in &reg.by_language {
                table.add_row(vec![
                    Cell::new(&language.language).fg(Color::Cyan),
                    Cell::new(language.status.to_string()),
                    Cell::new(format_optional_health(
                        language.from_grade,
                        language.from_score,
                    )),
                    Cell::new(format_optional_health(language.to_grade, language.to_score)),
                    Cell::new(format_optional_delta(language.score_delta)),
                ]);
            }
            writeln!(writer, "{table}")?;
            writeln!(writer)?;
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Before").add_attribute(Attribute::Bold),
            Cell::new("After").add_attribute(Attribute::Bold),
            Cell::new("Delta").add_attribute(Attribute::Bold),
        ]);
        let d = &report.delta;
        let rows = [
            ("Files", &d.files),
            ("Code", &d.code),
            ("Comments", &d.comment),
            ("Complexity", &d.complexity),
            ("Functions", &d.functions),
        ];
        for (label, dv) in rows {
            let signed = dv.signed_delta();
            let delta_cell = if signed > 0 {
                Cell::new(format!("+{signed}")).fg(Color::Yellow)
            } else if signed < 0 {
                Cell::new(signed.to_string()).fg(Color::Cyan)
            } else {
                Cell::new("0").fg(Color::DarkGrey)
            };
            table.add_row(vec![
                Cell::new(label),
                Cell::new(Self::format_number(dv.from)),
                Cell::new(Self::format_number(dv.to)),
                delta_cell,
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        let verdict = if reg.failed {
            "REGRESSED".red().bold().to_string()
        } else {
            "NO REGRESSION".green().bold().to_string()
        };
        writeln!(writer, "  Verdict: {verdict}")?;
        writeln!(writer)?;

        Ok(())
    }

    fn write_trend(
        &self,
        report: &crate::insight::trend::TrendReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        // Header
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Trend Report ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        let from_label = report.from.label.as_deref().unwrap_or_default();
        let to_label = report.to.label.as_deref().unwrap_or_default();
        writeln!(
            writer,
            "  {} {} {}  {} {} {}",
            "From:".bold(),
            report.from.timestamp.format("%Y-%m-%d"),
            from_label,
            "To:".bold(),
            report.to.timestamp.format("%Y-%m-%d"),
            to_label,
        )?;
        writeln!(writer)?;

        // Delta table
        let mut delta_table = Table::new();
        delta_table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        delta_table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Before").add_attribute(Attribute::Bold),
            Cell::new("After").add_attribute(Attribute::Bold),
            Cell::new("Delta").add_attribute(Attribute::Bold),
            Cell::new("Change").add_attribute(Attribute::Bold),
        ]);

        let deltas = [
            ("Files", &report.delta.files),
            ("Lines", &report.delta.lines),
            ("Code", &report.delta.code),
            ("Comments", &report.delta.comment),
            ("Blank", &report.delta.blank),
            ("Complexity", &report.delta.complexity),
            ("Functions", &report.delta.functions),
        ];

        for (name, dv) in &deltas {
            let signed = dv.signed_delta();
            let delta_color = if signed > 0 {
                Color::Green
            } else if signed < 0 {
                Color::Red
            } else {
                Color::White
            };
            let sign = if signed > 0 { "+" } else { "" };
            delta_table.add_row(vec![
                Cell::new(*name),
                Cell::new(Self::format_number(dv.from)),
                Cell::new(Self::format_number(dv.to)),
                Cell::new(format!("{sign}{signed}")).fg(delta_color),
                Cell::new(format!("{:+.1}%", dv.percent)).fg(delta_color),
            ]);
        }

        writeln!(writer, "{delta_table}")?;
        writeln!(writer)?;

        // By Language table
        if !report.by_language.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;
            let mut lang_table = Table::new();
            lang_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            lang_table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Status").add_attribute(Attribute::Bold),
                Cell::new("Before").add_attribute(Attribute::Bold),
                Cell::new("After").add_attribute(Attribute::Bold),
                Cell::new("Delta").add_attribute(Attribute::Bold),
            ]);

            for lang in &report.by_language {
                let signed = lang.code.signed_delta();
                let delta_color = if signed > 0 {
                    Color::Green
                } else if signed < 0 {
                    Color::Red
                } else {
                    Color::White
                };
                let sign = if signed > 0 { "+" } else { "" };
                lang_table.add_row(vec![
                    Cell::new(&lang.language).fg(Color::Cyan),
                    Cell::new(lang.status.to_string()),
                    Cell::new(Self::format_number(lang.code.from)),
                    Cell::new(Self::format_number(lang.code.to)),
                    Cell::new(format!("{sign}{signed}")).fg(delta_color),
                ]);
            }

            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        Ok(())
    }

    fn format_cost(cost: f64) -> String {
        if cost >= 1_000_000.0 {
            format!("{:.2}M", cost / 1_000_000.0)
        } else if cost >= 1_000.0 {
            format!("{:.0}", cost)
        } else {
            format!("{:.2}", cost)
        }
    }

    fn write_estimation(
        &self,
        report: &crate::insight::estimation::EstimationReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Cost Estimation Report ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(writer, "  Model: {}", report.model.bold())?;
        writeln!(writer)?;

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);
        table.add_row(vec![
            Cell::new("Total SLOC"),
            Cell::new(Self::format_number(report.total_sloc)).fg(Color::Cyan),
        ]);
        table.add_row(vec![
            Cell::new("Estimated Cost to Develop"),
            Cell::new(format!("${}", Self::format_cost(report.estimated_cost))).fg(Color::Green),
        ]);
        table.add_row(vec![
            Cell::new("Estimated Schedule Effort"),
            Cell::new(format!("{:.2} months", report.schedule_months)).fg(Color::Yellow),
        ]);
        table.add_row(vec![
            Cell::new("Estimated People Required"),
            Cell::new(format!("{:.2}", report.people_required)).fg(Color::Magenta),
        ]);
        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        if !options.summary_only && !report.by_language.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;
            let mut lang_table = Table::new();
            lang_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            lang_table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Effort (PM)").add_attribute(Attribute::Bold),
                Cell::new("Cost").add_attribute(Attribute::Bold),
            ]);
            let mut langs = report.by_language.iter().collect::<Vec<_>>();
            if let Some(n) = options.top_n {
                langs.truncate(n);
            }
            for lang in langs {
                lang_table.add_row(vec![
                    Cell::new(&lang.language).fg(Color::Cyan),
                    Cell::new(Self::format_number(lang.code_lines)),
                    Cell::new(format!("{:.2}", lang.effort_months)),
                    Cell::new(format!("${}", Self::format_cost(lang.cost))).fg(Color::Green),
                ]);
            }
            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        for (key, val) in &report.params {
            write!(writer, "{}  ", format!("{key}: {val}").dimmed())?;
        }
        writeln!(writer)?;

        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Cost Estimation Comparison ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(
            writer,
            "  Total SLOC: {}",
            Self::format_number(report.total_sloc).bold()
        )?;
        writeln!(
            writer,
            "  Scope: all counted SLOC, including tests and recognized documentation/configuration files"
        )?;
        writeln!(
            writer,
            "  Note: LOCOMO estimates LLM generation cost and review effort, not total software development cost"
        )?;
        writeln!(writer)?;

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Model").add_attribute(Attribute::Bold),
            Cell::new("Effort (PM)").add_attribute(Attribute::Bold),
            Cell::new("Schedule (M)").add_attribute(Attribute::Bold),
            Cell::new("People").add_attribute(Attribute::Bold),
            Cell::new("Cost").add_attribute(Attribute::Bold),
        ]);
        for r in &report.reports {
            table.add_row(vec![
                Cell::new(&r.model).fg(Color::Cyan),
                Cell::new(format!("{:.2}", r.effort_months)),
                Cell::new(format!("{:.2}", r.schedule_months)).fg(Color::Yellow),
                Cell::new(format!("{:.2}", r.people_required)).fg(Color::Magenta),
                Cell::new(format!("${}", Self::format_cost(r.estimated_cost))).fg(Color::Green),
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        // Per-model parameter summary
        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        for r in &report.reports {
            let params: Vec<String> = r.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
            writeln!(
                writer,
                "{}",
                format!("{}: {}", r.model, params.join(", ")).dimmed()
            )?;
        }

        Ok(())
    }

    fn grade_color(grade: crate::insight::Grade) -> Color {
        use crate::insight::Grade;
        match grade {
            Grade::A => Color::Green,
            Grade::B => Color::Cyan,
            Grade::C => Color::Yellow,
            Grade::D => Color::Red,
            Grade::F => Color::DarkRed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{AnalysisResult, FileStats, LineStats, Summary};
    use crate::insight::health::{
        FileRegression, HealthReport, LanguageHealthChange, LanguageHealthChangeStatus,
        RegressionReport,
    };
    use crate::insight::Grade;
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_result() -> AnalysisResult {
        let files = vec![FileStats {
            path: PathBuf::from("test.rs"),
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
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(5),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn test_strip_ansi_removes_csi_sequences() {
        let input = b"\x1b[1;32mgreen\x1b[0m plain";
        assert_eq!(strip_ansi(input), b"green plain");
    }

    #[test]
    fn test_no_ansi_codes_when_colorize_disabled() {
        let output = ConsoleOutput::new();
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buf = Vec::new();
        output
            .write(&Report::Analysis(make_result()), &options, &mut buf)
            .unwrap();
        assert!(
            !buf.contains(&0x1b),
            "colorize=false output must not contain ANSI escape codes"
        );
        // Content is still intact
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Rust"));
    }

    #[test]
    fn low_confidence_health_is_labeled_as_an_estimate() {
        let result = make_result();
        let model = crate::insight::scoring::default::DefaultModel::without_duplication();
        let report = crate::insight::health::score(&result, &model, 10);
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buffer = Vec::new();

        ConsoleOutput::new()
            .write(&Report::Health(report), &options, &mut buffer)
            .unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("Project Assessment: INSUFFICIENT DATA"));
        assert!(text.contains("Estimated Score:"));
        assert!(text.contains("Insufficient data"));
        assert!(!text.contains("Project Score:"));
    }

    #[test]
    fn test_health_comparison_uses_joint_model_score_and_marks_new_files() {
        let report = HealthReport {
            score: 99.0,
            grade: Grade::A,
            model: "default-v2".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![],
            scope: crate::insight::health::HealthScope::Production,
            confidence: Default::default(),
            assessment: Default::default(),
            tail_risk: Default::default(),
            by_language: vec![],
            test_health: None,
            regression: Some(RegressionReport {
                baseline: "snapshot old".to_string(),
                model: "default-v2-no-dup".to_string(),
                assessment: Default::default(),
                baseline_score: 93.3,
                baseline_grade: Grade::A,
                baseline_scope: crate::insight::health::HealthScope::TestsOnly,
                current_score: 92.0,
                current_grade: Grade::A,
                current_scope: crate::insight::health::HealthScope::Production,
                scope_changed: true,
                score_delta: -1.3,
                project_regressed: false,
                regressed_files: vec![FileRegression {
                    path: PathBuf::from("src/new_bad.rs"),
                    is_new: true,
                    from_score: 100.0,
                    from_grade: Grade::A,
                    to_score: 40.0,
                    to_grade: Grade::F,
                }],
                improved_files: 0,
                by_language: vec![LanguageHealthChange {
                    language: "Rust".to_string(),
                    status: LanguageHealthChangeStatus::Changed,
                    from_score: Some(93.3),
                    from_grade: Some(Grade::A),
                    to_score: Some(92.0),
                    to_grade: Some(Grade::A),
                    score_delta: Some(-1.3),
                }],
                failed: true,
            }),
        };
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buf = Vec::new();

        ConsoleOutput::new()
            .write(&Report::Health(report), &options, &mut buf)
            .unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Model: default-v2-no-dup"));
        assert!(text.contains("A (93.3) → A (92.0)"));
        assert!(text.contains("Scope changed: tests-only → production"));
        assert!(text.contains("Language Health Changes"));
        assert!(text.contains("Rust"));
        assert!(text.contains("-1.3"));
        assert!(text.contains("NEW"));
        assert!(!text.contains("A (93.3) → A (99.0)"));
    }
}
