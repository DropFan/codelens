use std::path::PathBuf;

use codelens_core::insight::coupling::{CouplingPair, CouplingReport};
use codelens_core::output::{ConsoleOutput, MarkdownOutput, OutputFormat, OutputOptions, Report};

fn coupling_fixture(with_pairs: bool) -> CouplingReport {
    let pairs = if with_pairs {
        vec![
            CouplingPair {
                file_a: PathBuf::from("src/a.rs"),
                file_b: PathBuf::from("src/b.rs"),
                shared_commits: 7,
                commits_a: 9,
                commits_b: 10,
                degree: 72.4,
            },
            CouplingPair {
                file_a: PathBuf::from("src/ui.rs"),
                file_b: PathBuf::from("src/data.rs"),
                shared_commits: 5,
                commits_a: 10,
                commits_b: 10,
                degree: 50.0,
            },
        ]
    } else {
        Vec::new()
    };

    CouplingReport {
        pairs,
        since: "90d".to_string(),
        total_commits: 1_234,
        skipped_large_commits: 2,
        excluded_test_files: 3,
        focus: Some(PathBuf::from("src/a.rs")),
    }
}

fn render(output: &dyn OutputFormat, report: CouplingReport) -> Vec<u8> {
    let options = OutputOptions {
        colorize: false,
        ..Default::default()
    };
    let mut buffer = Vec::new();
    output
        .write(&Report::Coupling(report), &options, &mut buffer)
        .unwrap();
    buffer
}

#[test]
fn coupling_human_outputs_match_byte_for_byte_baselines() {
    let cases: [(&dyn OutputFormat, bool, &str); 4] = [
        (
            &ConsoleOutput::new(),
            true,
            concat!(
                "\n",
                "════════════════════════════════════════════════════════════\n",
                " CODELENS - Change Coupling \n",
                "════════════════════════════════════════════════════════════\n",
                "\n",
                "  Period: 90d  Total Commits: 1,234\n",
                "  Focus: src/a.rs\n",
                "  2 bulk commit(s) excluded from pairing\n",
                "  3 test file(s) excluded (--include-tests to include)\n",
                "\n",
                "┌───────────┬─────────────┬───────────┬──────────┐\n",
                "│ File A    ┆ File B      ┆ Shared    ┆ Coupling │\n",
                "╞═══════════╪═════════════╪═══════════╪══════════╡\n",
                "│ src/a.rs  ┆ src/b.rs    ┆ 7 (9/10)  ┆ 72%      │\n",
                "├╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤\n",
                "│ src/ui.rs ┆ src/data.rs ┆ 5 (10/10) ┆ 50%      │\n",
                "└───────────┴─────────────┴───────────┴──────────┘\n",
                "\n",
                "  hint: Files changing together share a dependency the module\n",
                "  structure does not express — review the strongest pairs first.\n",
                "\n",
            ),
        ),
        (
            &ConsoleOutput::new(),
            false,
            concat!(
                "\n",
                "════════════════════════════════════════════════════════════\n",
                " CODELENS - Change Coupling \n",
                "════════════════════════════════════════════════════════════\n",
                "\n",
                "  Period: 90d  Total Commits: 1,234\n",
                "  Focus: src/a.rs\n",
                "  2 bulk commit(s) excluded from pairing\n",
                "  3 test file(s) excluded (--include-tests to include)\n",
                "\n",
                "  No coupled file pairs found.\n",
                "  (thresholds: --min-shared / --min-coupling can be lowered)\n",
            ),
        ),
        (
            &MarkdownOutput::new(),
            true,
            concat!(
                "# Change Coupling\n",
                "\n",
                "**Period:** 90d | **Total Commits:** 1234\n",
                "\n",
                "**Focus:** src/a.rs\n",
                "\n",
                "_2 bulk commit(s) excluded from pairing._\n",
                "\n",
                "_3 test file(s) excluded (`--include-tests` to include)._\n",
                "\n",
                "| File A | File B | Shared | Coupling |\n",
                "|--------|--------|--------|----------|\n",
                "| src/a.rs | src/b.rs | 7 (9/10) | 72% |\n",
                "| src/ui.rs | src/data.rs | 5 (10/10) | 50% |\n",
                "\n",
            ),
        ),
        (
            &MarkdownOutput::new(),
            false,
            concat!(
                "# Change Coupling\n",
                "\n",
                "**Period:** 90d | **Total Commits:** 1234\n",
                "\n",
                "**Focus:** src/a.rs\n",
                "\n",
                "_2 bulk commit(s) excluded from pairing._\n",
                "\n",
                "_3 test file(s) excluded (`--include-tests` to include)._\n",
                "\n",
                "No coupled file pairs found.\n",
            ),
        ),
    ];

    for (output, with_pairs, expected) in cases {
        assert_eq!(
            render(output, coupling_fixture(with_pairs)),
            expected.as_bytes()
        );
    }
}
