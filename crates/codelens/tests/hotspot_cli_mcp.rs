#![cfg(feature = "mcp")]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Codelens Test",
            "-c",
            "user.email=codelens@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

fn fixture() -> TempDir {
    let repo = tempfile::tempdir().expect("temporary repo should be created");
    git(repo.path(), &["init", "-q"]);
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(repo.path().join("outside")).unwrap();
    fs::write(
        repo.path().join(".codelens.toml"),
        "top = 2\nexclude_files = \"filtered[.]rs$\"\nthreads = 1\n",
    )
    .unwrap();

    for path in [
        "src/alpha.rs",
        "src/beta.rs",
        "src/gamma.rs",
        "src/filtered.rs",
    ] {
        fs::write(
            repo.path().join(path),
            "pub fn score(value: usize) -> usize {\n    if value > 10 { value * 2 } else { value + 1 }\n}\n",
        )
        .unwrap();
    }
    fs::write(
        repo.path().join("outside/ignored.rs"),
        "pub fn outside(value: usize) -> usize { value + 1 }\n",
    )
    .unwrap();
    commit_all(repo.path(), "initial files");

    for index in 0..4 {
        fs::write(
            repo.path().join("src/alpha.rs"),
            source("alpha", 10 + index, 2),
        )
        .unwrap();
        if index < 3 {
            fs::write(
                repo.path().join("src/beta.rs"),
                source("beta", 20 + index, 3),
            )
            .unwrap();
        }
        if index < 2 {
            fs::write(
                repo.path().join("src/gamma.rs"),
                source("gamma", 30 + index, 4),
            )
            .unwrap();
        }
        fs::write(
            repo.path().join("src/filtered.rs"),
            format!("pub fn filtered() -> usize {{ {index} }}\n"),
        )
        .unwrap();
        fs::write(
            repo.path().join("outside/ignored.rs"),
            format!("pub fn outside() -> usize {{ {index} }}\n"),
        )
        .unwrap();
        commit_all(repo.path(), &format!("change {index}"));
    }

    repo
}

fn source(name: &str, threshold: usize, multiplier: usize) -> String {
    format!(
        "pub fn {name}(value: usize) -> usize {{\n    if value > {threshold} {{ value * {multiplier} }} else {{ value + 1 }}\n}}\n"
    )
}

struct CliResult {
    report: Value,
    stderr: String,
}

fn run_cli(
    repo: &Path,
    path: &Path,
    since: &str,
    top: Option<usize>,
    functions: bool,
) -> CliResult {
    let mut command = Command::new(env!("CARGO_BIN_EXE_codelens"));
    command
        .current_dir(repo)
        .arg("hotspot")
        .arg(path)
        .args(["--since", since, "--format", "json"]);
    if let Some(top) = top {
        command.args(["--top", &top.to_string()]);
    }
    if functions {
        command.arg("--functions");
    }

    let output = command.output().expect("CLI should run");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    CliResult {
        report: serde_json::from_slice(&output.stdout).expect("CLI should return JSON"),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_mcp(repo: &Path, path: &Path, since: &str, top: Option<usize>, functions: bool) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codelens"))
        .arg("mcp")
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP server should start");

    let mut arguments = serde_json::Map::from_iter([
        ("path".to_string(), json!(path)),
        ("since".to_string(), json!(since)),
        ("functions".to_string(), json!(functions)),
    ]);
    if let Some(top) = top {
        arguments.insert("top".to_string(), json!(top));
    }

    {
        let stdin = child.stdin.as_mut().expect("MCP stdin should be piped");
        for message in [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "codelens-test", "version": "0" }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "hotspots", "arguments": arguments }
            }),
        ] {
            serde_json::to_writer(&mut *stdin, &message).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("MCP server should exit");
    assert!(
        output.status.success(),
        "MCP failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .find(|message| message.get("id") == Some(&json!(2)))
        .expect("MCP should return the tool response");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("hotspots should return text JSON");
    serde_json::from_str(text).expect("hotspots text should be a JSON report")
}

#[test]
fn cli_and_mcp_hotspot_json_match_for_scope_filters_top_and_functions() {
    let repo = fixture();
    let path = repo.path().join("src");

    let configured_cli = run_cli(repo.path(), &path, "2000-01-01", None, true);
    let configured_mcp = run_mcp(repo.path(), &path, "2000-01-01", None, true);
    assert_eq!(configured_cli.report, configured_mcp);
    assert_eq!(configured_cli.report["files"].as_array().unwrap().len(), 2);
    assert!(configured_cli.report["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| {
            let file_path = Path::new(file["path"].as_str().unwrap());
            file_path.starts_with(Path::new("src"))
                && file_path != Path::new("src/filtered.rs")
                && file["functions"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
                && file["age_days"].is_number()
                && file["knowledge"].is_object()
        }));

    let overridden_cli = run_cli(repo.path(), &path, "2000-01-01", Some(1), true);
    let overridden_mcp = run_mcp(repo.path(), &path, "2000-01-01", Some(1), true);
    assert_eq!(overridden_cli.report, overridden_mcp);
    assert_eq!(overridden_cli.report["files"].as_array().unwrap().len(), 1);
}

#[test]
fn empty_history_keeps_cli_hint_and_mcp_report_compatible() {
    let repo = fixture();
    let path = repo.path().join("src");

    let cli = run_cli(repo.path(), &path, "2030-01-01", None, false);
    let mcp = run_mcp(repo.path(), &path, "2030-01-01", None, false);

    assert_eq!(cli.report, mcp);
    assert_eq!(cli.report["total_commits"], 0);
    assert!(cli.report["files"].as_array().unwrap().is_empty());
    assert!(cli.stderr.contains("no commits found since '2030-01-01'"));
}
