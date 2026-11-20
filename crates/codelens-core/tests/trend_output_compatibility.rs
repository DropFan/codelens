use std::path::PathBuf;

use chrono::{DateTime, Utc};
use codelens_core::insight::trend::{
    LangStatus, LanguageTrend, SnapshotMeta, TrendDelta, TrendReport,
};
use codelens_core::insight::DeltaValue;
use codelens_core::output::{ConsoleOutput, MarkdownOutput, OutputFormat, OutputOptions, Report};

fn timestamp(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

fn trend_fixture(with_languages: bool) -> TrendReport {
    let by_language = if with_languages {
        vec![
            LanguageTrend {
                language: "Rust".to_string(),
                status: LangStatus::Changed,
                files: DeltaValue::new(8, 10),
                code: DeltaValue::new(600, 650),
            },
            LanguageTrend {
                language: "Python".to_string(),
                status: LangStatus::Removed,
                files: DeltaValue::new(4, 0),
                code: DeltaValue::new(200, 0),
            },
            LanguageTrend {
                language: "TypeScript".to_string(),
                status: LangStatus::Added,
                files: DeltaValue::new(0, 5),
                code: DeltaValue::new(0, 150),
            },
        ]
    } else {
        Vec::new()
    };

    TrendReport {
        from: SnapshotMeta {
            timestamp: timestamp("2026-10-01T12:00:00Z"),
            label: Some("baseline".to_string()),
            git_commit: None,
            file_path: PathBuf::from("before.json"),
        },
        to: SnapshotMeta {
            timestamp: timestamp("2026-11-20T08:30:00Z"),
            label: Some("current".to_string()),
            git_commit: None,
            file_path: PathBuf::from("after.json"),
        },
        delta: TrendDelta {
            files: DeltaValue::new(12, 15),
            lines: DeltaValue::new(1_000, 950),
            code: DeltaValue::new(800, 800),
            comment: DeltaValue::new(100, 120),
            blank: DeltaValue::new(100, 30),
            complexity: DeltaValue::new(90, 95),
            functions: DeltaValue::new(40, 38),
        },
        by_language,
        history: Vec::new(),
    }
}

fn render(output: &dyn OutputFormat, report: TrendReport) -> Vec<u8> {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };
    let mut buffer = Vec::new();
    output
        .write(&Report::Trend(report), &options, &mut buffer)
        .unwrap();
    buffer
}

#[test]
fn trend_human_outputs_match_byte_for_byte_baselines() {
    let cases: [(&dyn OutputFormat, bool, &[u8]); 4] = [
        (
            &ConsoleOutput::new(),
            true,
            include_bytes!("fixtures/trend-console-with-languages.txt"),
        ),
        (
            &ConsoleOutput::new(),
            false,
            include_bytes!("fixtures/trend-console-without-languages.txt"),
        ),
        (
            &MarkdownOutput::new(),
            true,
            include_bytes!("fixtures/trend-markdown-with-languages.md"),
        ),
        (
            &MarkdownOutput::new(),
            false,
            include_bytes!("fixtures/trend-markdown-without-languages.md"),
        ),
    ];

    for (output, with_languages, expected) in cases {
        assert_eq!(render(output, trend_fixture(with_languages)), expected);
    }
}
