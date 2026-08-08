//! Integration tests for insight features.

use std::path::PathBuf;
use std::time::Duration;

use codelens_core::analyzer::stats::{AnalysisResult, Complexity, FileStats, LineStats, Summary};
use codelens_core::insight::health;
use codelens_core::insight::hotspot;
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::insight::trend;

fn make_realistic_result() -> AnalysisResult {
    let files = vec![
        FileStats {
            path: PathBuf::from("src/main.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 120,
                code: 90,
                comment: 15,
                blank: 15,
            },
            size: 3000,
            complexity: Complexity {
                functions: 5,
                cyclomatic: 12,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 18.0,
            },
        },
        FileStats {
            path: PathBuf::from("src/parser/mod.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 350,
                code: 280,
                comment: 30,
                blank: 40,
            },
            size: 8000,
            complexity: Complexity {
                functions: 8,
                cyclomatic: 35,
                cognitive: 0,
                max_depth: 6,
                avg_func_lines: 35.0,
            },
        },
        FileStats {
            path: PathBuf::from("src/parser/expr.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 600,
                code: 500,
                comment: 40,
                blank: 60,
            },
            size: 15000,
            complexity: Complexity {
                functions: 12,
                cyclomatic: 60,
                cognitive: 0,
                max_depth: 8,
                avg_func_lines: 42.0,
            },
        },
        FileStats {
            path: PathBuf::from("src/utils.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 80,
                code: 60,
                comment: 10,
                blank: 10,
            },
            size: 2000,
            complexity: Complexity {
                functions: 6,
                cyclomatic: 8,
                cognitive: 0,
                max_depth: 2,
                avg_func_lines: 10.0,
            },
        },
        FileStats {
            path: PathBuf::from("tests/test_parser.py"),
            language: "Python".to_string(),
            lines: LineStats {
                total: 200,
                code: 150,
                comment: 20,
                blank: 30,
            },
            size: 4000,
            complexity: Complexity {
                functions: 10,
                cyclomatic: 15,
                cognitive: 0,
                max_depth: 3,
                avg_func_lines: 15.0,
            },
        },
    ];
    AnalysisResult {
        summary: Summary::from_file_stats(&files),
        files,
        elapsed: Duration::from_millis(50),
        scanned_files: 5,
        skipped_files: 0,
        error_files: 0,
    }
}

#[test]
fn test_health_end_to_end() {
    let result = make_realistic_result();
    let model = DefaultModel::new();
    let report = health::score(&result, &model, 10);

    assert!(report.score > 0.0 && report.score <= 100.0);
    assert_eq!(report.dimensions.len(), 5);
    assert!(!report.worst_files.is_empty());
    assert!(!report.by_directory.is_empty());
    // Worst file should be expr.rs (highest complexity, largest)
    assert_eq!(
        report.worst_files[0].path,
        PathBuf::from("src/parser/expr.rs")
    );
}

#[test]
fn test_hotspot_end_to_end() {
    let result = make_realistic_result();
    let churns = vec![
        codelens_core::git::FileChurn {
            path: PathBuf::from("src/parser/expr.rs"),
            commits: 45,
            lines_added: 800,
            lines_deleted: 300,
            last_commit_ts: 1_700_000_200,
        },
        codelens_core::git::FileChurn {
            path: PathBuf::from("src/main.rs"),
            commits: 10,
            lines_added: 50,
            lines_deleted: 20,
            last_commit_ts: 1_700_000_100,
        },
    ];

    let report = hotspot::analyze(&churns, &result, "90d", 200, 10);
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.files[0].path, PathBuf::from("src/parser/expr.rs"));
}

#[test]
fn test_trend_save_and_diff() {
    let dir = tempfile::TempDir::new().unwrap();

    let result1 = make_realistic_result();
    trend::save_snapshot(dir.path(), result1, Some("v1".into()), None, None).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1100));

    let mut result2 = make_realistic_result();
    result2.files.push(FileStats {
        path: PathBuf::from("src/new_feature.rs"),
        language: "Rust".to_string(),
        lines: LineStats {
            total: 200,
            code: 160,
            comment: 20,
            blank: 20,
        },
        size: 4000,
        complexity: Complexity {
            functions: 8,
            cyclomatic: 16,
            cognitive: 0,
            max_depth: 3,
            avg_func_lines: 20.0,
        },
    });
    result2.summary = Summary::from_file_stats(&result2.files);
    result2.scanned_files = 6;
    trend::save_snapshot(dir.path(), result2, Some("v2".into()), None, None).unwrap();

    let report = trend::diff(dir.path(), "latest~1", "latest").unwrap();
    assert_eq!(report.delta.files.from, 5);
    assert_eq!(report.delta.files.to, 6);
    assert!(report.delta.code.signed_delta() > 0);
}

#[test]
fn test_health_json_roundtrip() {
    let result = make_realistic_result();
    let model = DefaultModel::new();
    let report = health::score(&result, &model, 10);

    let json = serde_json::to_string(&report).unwrap();
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_output_format_with_reports() {
    use codelens_core::output::{JsonOutput, OutputFormat, OutputOptions, Report};

    let result = make_realistic_result();
    let model = DefaultModel::new();
    let health_report = health::score(&result, &model, 10);

    let json_output = JsonOutput::new(true);
    let options = OutputOptions::default();
    let mut buffer = Vec::new();
    json_output
        .write(&Report::Health(health_report), &options, &mut buffer)
        .unwrap();
    let json_str = String::from_utf8(buffer).unwrap();
    assert!(json_str.contains("\"score\""));
    assert!(json_str.contains("\"grade\""));
}
