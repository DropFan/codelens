use std::path::PathBuf;

use codelens_core::analyzer::stats::Complexity;
use codelens_core::insight::hotspot::{
    ChurnMetrics, FileHotspot, FunctionHotspot, HotspotReport, KnowledgeMetrics, RiskLevel,
};
use codelens_core::output::{ConsoleOutput, MarkdownOutput, OutputFormat, OutputOptions, Report};

fn hotspot_fixture() -> HotspotReport {
    HotspotReport {
        files: vec![
            FileHotspot {
                path: PathBuf::from("src/owned.rs"),
                language: "Rust".to_string(),
                churn: ChurnMetrics {
                    commits: 12,
                    lines_added: 345,
                    lines_deleted: 67,
                    lines_churn: 412,
                },
                complexity: Complexity {
                    cyclomatic: 18,
                    ..Default::default()
                },
                hotspot_score: 0.72,
                risk: RiskLevel::High,
                age_days: Some(400),
                knowledge: Some(KnowledgeMetrics {
                    authors: 1,
                    main_author: "Alice".to_string(),
                    ownership: 0.8,
                    knowledge_island: true,
                }),
                functions: Some(vec![FunctionHotspot {
                    name: "run".to_string(),
                    start_line: 10,
                    end_line: 20,
                    commits: 5,
                    cyclomatic: 3,
                }]),
            },
            FileHotspot {
                path: PathBuf::from("src/shared.rs"),
                language: "Rust".to_string(),
                churn: ChurnMetrics {
                    commits: 2,
                    lines_added: 3,
                    lines_deleted: 1,
                    lines_churn: 4,
                },
                complexity: Complexity {
                    cyclomatic: 2,
                    ..Default::default()
                },
                hotspot_score: 0.2,
                risk: RiskLevel::Low,
                age_days: None,
                knowledge: Some(KnowledgeMetrics {
                    authors: 3,
                    main_author: "Bob".to_string(),
                    ownership: 0.5,
                    knowledge_island: false,
                }),
                functions: Some(vec![]),
            },
        ],
        since: "90d".to_string(),
        total_commits: 1_234,
    }
}

fn render(output: &dyn OutputFormat, report: HotspotReport) -> String {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };
    let mut buffer = Vec::new();
    output
        .write(&Report::Hotspot(report), &options, &mut buffer)
        .unwrap();
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
fn hotspot_human_outputs_keep_content_order_and_formatting() {
    let console = render(&ConsoleOutput::new(), hotspot_fixture());
    assert_in_order(
        &console,
        &[
            "CODELENS - Hotspot Analysis",
            "Period: 90d  Total Commits: 1,234",
            "src/owned.rs",
            "+345/-67",
            "1.1y",
            "1 ★",
            "0.72",
            "HIGH",
            "src/shared.rs",
            "+3/-1",
            "3",
            "0.20",
            "LOW",
            "★ Knowledge Islands",
            "src/owned.rs  80% by Alice",
            "Function Hotspots (approximate spans)",
            "src/owned.rs",
            "run",
            "L10-20",
            "5 commits  CC 3",
            "src/shared.rs",
        ],
    );
    assert!(!console.contains('\u{1b}'));

    let markdown = render(&MarkdownOutput::new(), hotspot_fixture());
    assert_eq!(
        markdown,
        concat!(
            "# Hotspot Analysis\n",
            "\n",
            "**Period:** 90d | **Total Commits:** 1234\n",
            "\n",
            "| File | Chg | +/- | CC | Age | Authors | Score | Risk |\n",
            "|------|-----|-----|----|-----|---------|-------|------|\n",
            "| src/owned.rs | 12 | +345/-67 | 18 | 1.1y | 1 ★ | 0.72 | HIGH |\n",
            "| src/shared.rs | 2 | +3/-1 | 2 | - | 3 | 0.20 | LOW |\n",
            "\n",
            "**★ Knowledge Islands** (risky files effectively one person knows):\n",
            "\n",
            "- src/owned.rs — 80% by Alice\n",
            "\n",
            "## Function Hotspots (approximate spans)\n",
            "\n",
            "| File | Function | Lines | Commits | CC |\n",
            "|------|----------|-------|---------|-----|\n",
            "| src/owned.rs | run | 10-20 | 5 | 3 |\n",
            "\n",
        )
    );
}

#[test]
fn empty_hotspot_human_outputs_are_unchanged() {
    let report = HotspotReport {
        files: vec![],
        since: "90d".to_string(),
        total_commits: 0,
    };

    let console = render(&ConsoleOutput::new(), report.clone());
    assert_eq!(
        console,
        concat!(
            "\n",
            "════════════════════════════════════════════════════════════\n",
            " CODELENS - Hotspot Analysis \n",
            "════════════════════════════════════════════════════════════\n",
            "\n",
            "  Period: 90d  Total Commits: 0\n",
            "\n",
            "  No hotspots found.\n",
        )
    );

    let markdown = render(&MarkdownOutput::new(), report);
    assert_eq!(
        markdown,
        concat!(
            "# Hotspot Analysis\n",
            "\n",
            "**Period:** 90d | **Total Commits:** 0\n",
            "\n",
            "No hotspots found.\n",
        )
    );
}
