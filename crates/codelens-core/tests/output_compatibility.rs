use std::path::PathBuf;

use codelens_core::insight::health::{FileHealth, HealthReport, HealthScope};
use codelens_core::insight::scoring::HealthDimension;
use codelens_core::insight::Grade;
use codelens_core::output::{CsvOutput, OutputFormat, OutputOptions, Report};

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
