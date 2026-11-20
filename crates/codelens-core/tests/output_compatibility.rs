use std::path::PathBuf;
use std::time::Duration;

use codelens_core::analyzer::stats::{AnalysisResult, Complexity, FileStats, LineStats, Summary};
use codelens_core::insight::diff::DiffReport;
use codelens_core::insight::health::{
    Confidence, DimensionScore, DirectoryHealth, FileHealth, FileRegression, HealthAssessment,
    HealthReport, HealthScope, LanguageHealth, LanguageHealthChange, LanguageHealthChangeStatus,
    RegressionReport, TailRisk,
};
use codelens_core::insight::scoring::HealthDimension;
use codelens_core::insight::trend::{LangStatus, LanguageTrend, TrendDelta};
use codelens_core::insight::{DeltaValue, Grade};
use codelens_core::output::{
    ConsoleOutput, CsvOutput, HtmlOutput, MarkdownOutput, OutputFormat, OutputOptions, Report,
};

fn analysis_fixture() -> AnalysisResult {
    let files = vec![
        FileStats {
            path: PathBuf::from("src/lib.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 1_500,
                code: 1_234,
                comment: 123,
                blank: 143,
            },
            size: 2_048,
            complexity: Complexity {
                functions: 12,
                cyclomatic: 34,
                cognitive: 56,
                ..Default::default()
            },
            ..Default::default()
        },
        FileStats {
            path: PathBuf::from("tests/check.py"),
            language: "Python".to_string(),
            lines: LineStats {
                total: 20,
                code: 12,
                comment: 3,
                blank: 5,
            },
            size: 512,
            complexity: Complexity {
                functions: 2,
                cyclomatic: 3,
                cognitive: 4,
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    let mut summary = Summary::from_file_stats(&files);
    summary.uloc = 1_200;
    summary.dup_scanned = true;

    AnalysisResult {
        summary,
        files,
        elapsed: Duration::from_millis(1250),
        scanned_files: 3,
        skipped_files: 1,
        error_files: 0,
    }
}

fn health_fixture() -> HealthReport {
    HealthReport {
        score: 82.14,
        grade: Grade::B,
        model: "default-v2".to_string(),
        dimensions: vec![DimensionScore {
            dimension: HealthDimension::Complexity,
            score: 64.66,
            grade: Grade::D,
            weight: 0.3,
        }],
        by_directory: vec![DirectoryHealth {
            path: PathBuf::from("src/output"),
            score: 61.26,
            grade: Grade::D,
            file_count: 4,
        }],
        worst_files: vec![FileHealth {
            path: PathBuf::from("src/output/console.rs"),
            score: 45.64,
            grade: Grade::F,
            top_issue: HealthDimension::Complexity,
            dimensions: vec![],
        }],
        scope: HealthScope::Production,
        confidence: Confidence::from_coverage(0.876),
        assessment: Default::default(),
        tail_risk: TailRisk {
            tail_score: 43.26,
            failing_files: 1,
            failing_ratio: 0.25,
            worst_score: 40.0,
        },
        by_language: vec![LanguageHealth {
            language: "Rust".to_string(),
            score: 71.24,
            grade: Grade::C,
            file_count: 4,
            code_lines: 1_234,
            confidence: Confidence::from_coverage(0.876),
            assessment: Default::default(),
            dimensions: vec![],
            tail_risk: TailRisk {
                tail_score: 45.64,
                ..Default::default()
            },
        }],
        test_health: None,
        regression: None,
    }
}

fn diff_fixture() -> DiffReport {
    DiffReport {
        from: "main".to_string(),
        to: "worktree".to_string(),
        to_score: 73.0,
        to_grade: Grade::C,
        health: RegressionReport {
            baseline: "main".to_string(),
            model: "default-v2".to_string(),
            assessment: HealthAssessment::Scored,
            baseline_score: 80.0,
            baseline_grade: Grade::B,
            baseline_scope: HealthScope::TestsOnly,
            current_score: 73.0,
            current_grade: Grade::C,
            current_scope: HealthScope::Production,
            scope_changed: true,
            score_delta: -7.0,
            project_regressed: true,
            regressed_files: vec![
                FileRegression {
                    path: PathBuf::from("src/lib.rs"),
                    is_new: false,
                    from_score: 80.0,
                    from_grade: Grade::B,
                    to_score: 60.0,
                    to_grade: Grade::D,
                },
                FileRegression {
                    path: PathBuf::from("src/new.rs"),
                    is_new: true,
                    from_score: 100.0,
                    from_grade: Grade::A,
                    to_score: 40.0,
                    to_grade: Grade::F,
                },
            ],
            improved_files: 2,
            by_language: vec![
                LanguageHealthChange {
                    language: "Rust".to_string(),
                    status: LanguageHealthChangeStatus::Changed,
                    from_score: Some(80.0),
                    from_grade: Some(Grade::B),
                    to_score: Some(73.0),
                    to_grade: Some(Grade::C),
                    score_delta: Some(-7.0),
                },
                LanguageHealthChange {
                    language: "TypeScript".to_string(),
                    status: LanguageHealthChangeStatus::Added,
                    from_score: None,
                    from_grade: None,
                    to_score: Some(90.0),
                    to_grade: Some(Grade::A),
                    score_delta: None,
                },
            ],
            failed: true,
        },
        delta: TrendDelta {
            files: DeltaValue::new(10, 12),
            lines: DeltaValue::new(1_500, 1_610),
            code: DeltaValue::new(1_200, 1_325),
            comment: DeltaValue::new(150, 140),
            blank: DeltaValue::new(150, 145),
            complexity: DeltaValue::new(80, 75),
            functions: DeltaValue::new(60, 60),
        },
        by_language: vec![
            LanguageTrend {
                language: "Rust".to_string(),
                status: LangStatus::Changed,
                files: DeltaValue::new(8, 9),
                code: DeltaValue::new(1_000, 1_150),
            },
            LanguageTrend {
                language: "Python".to_string(),
                status: LangStatus::Changed,
                files: DeltaValue::new(2, 2),
                code: DeltaValue::new(200, 175),
            },
        ],
    }
}

fn render(output: &dyn OutputFormat, report: Report, options: &OutputOptions) -> String {
    let mut buffer = Vec::new();
    output.write(&report, options, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

fn assert_in_order(haystack: &str, needles: &[&str]) {
    let mut offset = 0;
    for needle in needles {
        let relative = haystack[offset..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing output fragment: {needle:?}"));
        offset += relative + needle.len();
    }
}

#[test]
fn analysis_human_outputs_keep_key_fields_in_order() {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };

    let console = render(
        &ConsoleOutput::new(),
        Report::Analysis(analysis_fixture()),
        &options,
    );
    assert_in_order(
        &console,
        &[
            "Total Files",
            "Code Lines",
            "Comment Lines",
            "Blank Lines",
            "Total Lines",
            "Total Size",
            "Languages",
            "Functions",
            "Complexity (CC / Cognitive)",
            "Test Files",
            "Test Code Lines",
            "Test/Code Ratio",
            "ULOC / DRYness",
            "By Language",
            "Rust",
            "Python",
        ],
    );
    assert!(console.contains("1,234"));
    assert!(console.contains("37 / 60"));
    assert!(console.contains("1,200 / 87%"));
    assert!(console.contains("Scanned 3 files in 1.25s (1 skipped)"));

    let markdown = render(
        &MarkdownOutput::new(),
        Report::Analysis(analysis_fixture()),
        &options,
    );
    assert!(markdown.contains(
        "| Total Files | 2 |\n| Code Lines | 1246 |\n| Comment Lines | 126 |\n| Blank Lines | 148 |\n| Total Lines | 1520 |\n| Languages | 2 |"
    ));
    assert!(markdown
        .contains("| Test Files | 1 |\n| Test Code Lines | 12 |\n| Test/Code Ratio | 0.01 |"));
    assert_in_order(&markdown, &["## By Language", "| Rust |", "| Python |"]);
    assert!(markdown.ends_with("*Generated by codelens in 1.25s*\n"));

    let html = render(
        &HtmlOutput::new(),
        Report::Analysis(analysis_fixture()),
        &options,
    );
    assert_in_order(
        &html,
        &[
            ">2</div>\n                <div class=\"label\">Code Files",
            ">1246</div>\n                <div class=\"label\">Code Lines",
            ">126</div>\n                <div class=\"label\">Comment Lines",
            ">148</div>\n                <div class=\"label\">Blank Lines",
            ">2</div>\n                <div class=\"label\">Languages",
            ">14</div>\n                <div class=\"label\">Functions",
            "<td><strong>Rust</strong></td>",
            "<td><strong>Python</strong></td>",
            "in 1.25s",
        ],
    );
}

#[test]
fn analysis_optional_fields_stay_absent_for_default_values() {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };
    let empty = AnalysisResult {
        summary: Summary::default(),
        files: vec![],
        elapsed: Duration::ZERO,
        scanned_files: 0,
        skipped_files: 0,
        error_files: 0,
    };

    let console = render(
        &ConsoleOutput::new(),
        Report::Analysis(empty.clone()),
        &options,
    );
    let markdown = render(
        &MarkdownOutput::new(),
        Report::Analysis(empty.clone()),
        &options,
    );
    let html = render(&HtmlOutput::new(), Report::Analysis(empty), &options);

    for output in [&console, &markdown] {
        assert!(!output.contains("Test Files"));
        assert!(!output.contains("Test/Code Ratio"));
        assert!(!output.contains("ULOC / DRYness"));
        assert!(!output.contains("By Language"));
    }
    assert!(!html.contains("<td><strong>"));
}

#[test]
fn health_human_outputs_keep_fields_precision_and_defaults() {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };

    let console = render(
        &ConsoleOutput::new(),
        Report::Health(health_fixture()),
        &options,
    );
    assert_in_order(
        &console,
        &[
            "Project Score: 82.1  Grade: B  Model: v2",
            "Health Scope: Production code  Measurement Confidence: high (87.6% measured)",
            "Worst 10% File Average: 43.3  F-grade Files: 1 (25.0%)",
            "Worst Directory: src/output  D (61.3)  4 files",
            "Risk: 1 file(s) are graded F despite the overall B grade.",
            "Complexity",
            "64.7",
            "By Language",
            "Rust",
            "71.2",
            "By Directory",
            "src/output",
            "61.3",
            "Worst Files",
            "src/output/console.rs",
            "45.6",
        ],
    );
    assert!(!console.contains("Test Code Health:"));

    let markdown = render(
        &MarkdownOutput::new(),
        Report::Health(health_fixture()),
        &options,
    );
    assert!(markdown.contains(
        "**Project Score:** 82.1 | **Grade:** B | **Model:** v2\n**Health analysis scope:** Production code | **Measurement confidence:** high (87.6% measured)\n**Worst 10% file average:** 43.3 | **F-grade files:** 1 (25.0%)"
    ));
    assert_in_order(
        &markdown,
        &[
            "| Complexity | 64.7 | D |",
            "| Rust | 71.2 | C | 4 | 1234 | high (88%) | 45.6 |",
            "| src/output | 61.3 | D | 4 |",
            "| src/output/console.rs | 45.6 | F | Complexity |",
        ],
    );
    assert!(!markdown.contains("**Test code health:**"));

    let html = render(
        &HtmlOutput::new(),
        Report::Health(health_fixture()),
        &options,
    );
    assert_in_order(
        &html,
        &[
            "<strong>Score: 82.1 / 100</strong> | Model: v2",
            "Health analysis scope: Production code | Measurement confidence: high (87.6% measured)",
            "Worst 10% file average: 43.3 | F-grade files: 1 (25.0%)",
            ">64.7</div>",
            "<strong>Rust</strong></td><td>71.2</td><td>C</td><td>4</td><td>1234</td><td>high (88%)</td><td>45.6</td>",
            "<strong>src/output</strong></td><td>61.3</td><td>D</td><td>4</td>",
            "<strong>src/output/console.rs</strong></td><td>45.6</td><td>F</td><td>Complexity</td>",
        ],
    );
    assert!(!html.contains("Test code health:"));
}

#[test]
fn health_csv_schema_is_unchanged() {
    let report = HealthReport {
        score: 87.4,
        grade: Grade::B,
        model: "default".to_string(),
        dimensions: vec![],
        by_directory: vec![],
        worst_files: vec![FileHealth {
            path: PathBuf::from("src/lib.rs"),
            score: 43.64,
            grade: Grade::F,
            top_issue: HealthDimension::Complexity,
            dimensions: vec![],
        }],
        scope: HealthScope::LegacyAll,
        confidence: Default::default(),
        assessment: Default::default(),
        tail_risk: Default::default(),
        by_language: vec![],
        test_health: None,
        regression: None,
    };
    let mut buffer = Vec::new();

    CsvOutput::new()
        .write(
            &Report::Health(report),
            &OutputOptions::default(),
            &mut buffer,
        )
        .unwrap();

    assert_eq!(
        String::from_utf8(buffer).unwrap(),
        "File,Score,Grade,TopIssue\nsrc/lib.rs,43.6,F,Complexity\n"
    );
}

#[test]
fn diff_console_and_markdown_keep_content_order_and_formatting() {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };

    let console = render(
        &ConsoleOutput::new(),
        Report::Diff(Box::new(diff_fixture())),
        &options,
    );
    assert_in_order(
        &console,
        &[
            "CODELENS - Diff Report",
            "main → worktree",
            "Health [default-v2]: B (80.0) → C (73.0)  Δ -7.0",
            "Scope changed: tests-only → production (project delta is informational)",
            "Improved files: 2",
            "Regressed Files",
            "src/lib.rs",
            "src/new.rs",
            "Language Health Changes",
            "Rust",
            "TypeScript",
            "Metric",
            "Files",
            "Code",
            "Comments",
            "Complexity",
            "Functions",
            "Verdict: REGRESSED",
        ],
    );
    assert!(console.contains("1,200"));
    assert!(console.contains("1,325"));
    assert!(!console.contains('\u{1b}'));

    let markdown = render(
        &MarkdownOutput::new(),
        Report::Diff(Box::new(diff_fixture())),
        &options,
    );
    assert_eq!(
        markdown,
        concat!(
            "# Diff Report: main → worktree\n",
            "\n",
            "**Health [default-v2]:** B (80.0) → C (73.0), Δ -7.0 — 🔴 REGRESSED\n",
            "**Scope changed:** tests-only → production (project delta is informational and does not affect the gate)\n",
            "\n",
            "## Regressed Files\n",
            "\n",
            "| File | Before | After |\n",
            "|------|--------|-------|\n",
            "| src/lib.rs | B (80.0) | D (60.0) |\n",
            "| src/new.rs | NEW | F (40.0) |\n",
            "\n",
            "## Language Health Changes\n",
            "\n",
            "| Language | Status | Before | After | Delta |\n",
            "|----------|--------|--------|-------|-------|\n",
            "| Rust | changed | B (80.0) | C (73.0) | -7.0 |\n",
            "| TypeScript | added | - | A (90.0) | - |\n",
            "\n",
            "## Metrics\n",
            "\n",
            "| Metric | Before | After | Delta |\n",
            "|--------|--------|-------|-------|\n",
            "| Files | 10 | 12 | +2 |\n",
            "| Code | 1200 | 1325 | +125 |\n",
            "| Comments | 150 | 140 | -10 |\n",
            "| Complexity | 80 | 75 | -5 |\n",
            "| Functions | 60 | 60 | +0 |\n",
            "\n",
            "## By Language\n",
            "\n",
            "| Language | Code Before | Code After | Delta |\n",
            "|----------|-------------|------------|-------|\n",
            "| Rust | 1000 | 1150 | +150 |\n",
            "| Python | 200 | 175 | -25 |\n",
            "\n",
        )
    );
}

#[test]
fn diff_optional_groups_stay_absent_and_estimates_keep_the_existing_verdict() {
    let mut report = diff_fixture();
    report.health.assessment = HealthAssessment::InsufficientData;
    report.health.baseline_score = 73.0;
    report.health.baseline_grade = Grade::C;
    report.health.current_score = 74.5;
    report.health.current_grade = Grade::C;
    report.health.scope_changed = false;
    report.health.score_delta = 1.5;
    report.health.project_regressed = false;
    report.health.regressed_files.clear();
    report.health.improved_files = 0;
    report.health.by_language.clear();
    report.health.failed = false;
    report.by_language.clear();
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };

    for output in [
        render(
            &ConsoleOutput::new(),
            Report::Diff(Box::new(report.clone())),
            &options,
        ),
        render(
            &MarkdownOutput::new(),
            Report::Diff(Box::new(report)),
            &options,
        ),
    ] {
        assert!(output.contains("insufficient data (score delta is an estimate)"));
        assert!(output.contains("NO REGRESSION"));
        assert!(!output.contains("Regressed Files"));
        assert!(!output.contains("Language Health Changes"));
        assert!(!output.contains("By Language"));
        assert!(!output.contains("Improved files"));
    }
}
