use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

use codelens_core::config::OutputFormatType;
use codelens_core::git;
use codelens_core::insight::coupling::{self, CouplingOptions};
use codelens_core::insight::estimation;
use codelens_core::insight::health;
use codelens_core::insight::hotspot;
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::output::{create_output, CombinedReport, HtmlOutput, OutputOptions, Report};
use codelens_core::{
    analyze, CocomoBasicModel, CocomoIIModel, Config, CostConfig, EstimationModel, GitClient,
    LocomoModel, PutnamModel,
};

const SOURCE_FILES: usize = 120;
const HISTORY_COMMITS: usize = 24;

struct WorkflowFixture {
    _repository: TempDir,
    root: PathBuf,
    config: Config,
    git_client: GitClient,
    combined: CombinedReport,
    hotspot: hotspot::HotspotReport,
    coupling: coupling::CouplingReport,
}

impl WorkflowFixture {
    fn new() -> Self {
        let repository = tempfile::tempdir().expect("benchmark repository");
        git(repository.path(), &["init", "-q"]);
        let source_root = repository.path().join("src");
        fs::create_dir(&source_root).expect("source directory");

        for index in 0..SOURCE_FILES {
            let module = source_root.join(format!("module_{:02}", index % 12));
            fs::create_dir_all(&module).expect("module directory");
            fs::write(
                module.join(format!("file_{index:03}.rs")),
                rust_source(index, 80),
            )
            .expect("source fixture");
        }
        commit_all(repository.path(), "initial fixture");

        for commit in 0..HISTORY_COMMITS {
            for offset in 0..8 {
                let index = (commit * 5 + offset * 7) % SOURCE_FILES;
                let path = source_root
                    .join(format!("module_{:02}", index % 12))
                    .join(format!("file_{index:03}.rs"));
                let mut file = OpenOptions::new()
                    .append(true)
                    .open(path)
                    .expect("fixture file");
                writeln!(
                    file,
                    "pub fn change_{commit}_{offset}() -> usize {{ {commit} }}"
                )
                .expect("append fixture change");
            }
            commit_all(repository.path(), &format!("history {commit:02}"));
        }

        let mut config = Config::default();
        config.walker.threads = 1;
        config.filter.smart_exclude = true;
        let mut analysis = analyze(&[repository.path()], &config).expect("fixture analysis");
        make_repo_relative(&mut analysis.files, repository.path());
        let regions = std::mem::take(&mut analysis.summary.health_regions);
        analysis.summary.health_regions = regions
            .into_iter()
            .map(|(path, stats)| {
                let relative = path
                    .strip_prefix(repository.path())
                    .unwrap_or(&path)
                    .to_path_buf();
                (relative, stats)
            })
            .collect();

        let git_client = GitClient::detect(repository.path()).expect("git fixture");
        let since = git::parse_since("2000-01-01");
        let commits = git_client.commit_log(&since).expect("commit history");
        let churn = git::churn_from_commits(&commits);
        let hotspot = hotspot::analyze(&churn, &analysis, "2000-01-01", commits.len(), 20);
        let universe = analysis
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let coupling = coupling::analyze(
            &commits,
            Some(&universe),
            "2000-01-01",
            &CouplingOptions {
                min_shared: 1,
                min_degree: 0.0,
                ..CouplingOptions::default()
            },
        );
        let health = health::score(&analysis, &DefaultModel::default(), 10);

        let cost = CostConfig::default();
        let cocomo_basic = CocomoBasicModel::default();
        let cocomo2 = CocomoIIModel::default();
        let putnam = PutnamModel::default();
        let locomo = LocomoModel::default();
        let models: Vec<&dyn EstimationModel> = vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
        let estimation = estimation::estimate_all(&analysis.summary, &models, &cost);
        let combined = CombinedReport {
            analysis,
            health,
            estimation,
        };

        Self {
            _repository: repository,
            root: source_root,
            config,
            git_client,
            combined,
            hotspot,
            coupling,
        }
    }
}

fn rust_source(file: usize, functions: usize) -> String {
    let mut source = String::new();
    for function in 0..functions {
        source.push_str(&format!(
            "pub fn file_{file}_function_{function}(value: usize) -> usize {{\n    if value % 2 == 0 {{ value + {function} }} else {{ value }}\n}}\n"
        ));
    }
    source
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git executable");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=Codelens Benchmark",
            "-c",
            "user.email=codelens-benchmark@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

fn make_repo_relative(files: &mut [codelens_core::FileStats], root: &Path) {
    for file in files {
        if let Ok(relative) = file.path.strip_prefix(root) {
            file.path = relative.to_path_buf();
        }
    }
}

fn render_health_outputs(report: &Report) -> usize {
    [
        OutputFormatType::Markdown,
        OutputFormatType::Json,
        OutputFormatType::Badge,
    ]
    .into_iter()
    .map(|format| {
        let mut output = Vec::new();
        create_output(format)
            .write(report, &OutputOptions::default(), &mut output)
            .expect("render health output");
        output.len()
    })
    .sum()
}

fn bench_workflows(c: &mut Criterion) {
    let fixture = WorkflowFixture::new();
    let mut group = c.benchmark_group("workflow");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    group.bench_function("end_to_end_scan", |b| {
        b.iter(|| {
            black_box(
                analyze(std::slice::from_ref(&fixture.root), &fixture.config)
                    .expect("benchmark analysis"),
            )
        });
    });

    group.bench_function("git_history_25_commits", |b| {
        let since = git::parse_since("2000-01-01");
        b.iter(|| {
            black_box(
                fixture
                    .git_client
                    .commit_log(&since)
                    .expect("benchmark git history"),
            )
        });
    });

    group.bench_function("html_dashboard", |b| {
        b.iter(|| {
            let mut output = Vec::with_capacity(256 * 1024);
            HtmlOutput::new()
                .write_combined_dashboard(
                    &fixture.combined,
                    Some(&fixture.hotspot),
                    Some(&fixture.coupling),
                    None,
                    "",
                    "",
                    &OutputOptions::default(),
                    &mut output,
                )
                .expect("render HTML dashboard");
            black_box(output.len())
        });
    });

    group.bench_function("health_outputs_three_scans", |b| {
        b.iter(|| {
            let mut bytes = 0;
            for format in [
                OutputFormatType::Markdown,
                OutputFormatType::Json,
                OutputFormatType::Badge,
            ] {
                let analysis =
                    analyze(std::slice::from_ref(&fixture.root), &fixture.config).unwrap();
                let health = health::score(&analysis, &DefaultModel::default(), 10);
                let report = Report::Health(health);
                let mut output = Vec::new();
                create_output(format)
                    .write(&report, &OutputOptions::default(), &mut output)
                    .unwrap();
                bytes += output.len();
            }
            black_box(bytes)
        });
    });

    group.bench_function("health_outputs_one_scan", |b| {
        b.iter(|| {
            let analysis = analyze(std::slice::from_ref(&fixture.root), &fixture.config).unwrap();
            let health = health::score(&analysis, &DefaultModel::default(), 10);
            black_box(render_health_outputs(&Report::Health(health)))
        });
    });

    group.finish();
}

criterion_group!(benches, bench_workflows);
criterion_main!(benches);
