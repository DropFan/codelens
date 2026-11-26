#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn health_step_script() -> String {
    let action = include_str!("../../../action.yml");
    let step = action
        .split("    - name: Run codelens health\n")
        .nth(1)
        .expect("health action step");
    let body = step
        .split("      run: |\n")
        .nth(1)
        .expect("health action run script");

    body.lines()
        .take_while(|line| !line.starts_with("    - name:"))
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fake_codelens(path: &Path) {
    fs::write(
        path,
        r#"#!/usr/bin/env bash
set -e
printf '%s\n' "$*" >> "$FAKE_LOG"
if [ "${1:-}" = "health" ] && [ "${2:-}" = "--help" ]; then
  if [ "${FAKE_SUPPORTS_EMIT:-0}" = "1" ]; then
    echo '  --emit <FORMAT=FILE>'
  fi
  exit 0
fi

format=console
output=
emits=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    -f|--format) format="$2"; shift 2 ;;
    -O|--output-file) output="$2"; shift 2 ;;
    --emit) emits+=("$2"); shift 2 ;;
    *) shift ;;
  esac
done

write_output() {
  local selected="$1"
  local destination="$2"
  case "$selected" in
    markdown) printf '# Code Health Report\n' > "$destination" ;;
    json) printf '{"score":91.5,"grade":"A","assessment":"scored","model":"default"}\n' > "$destination" ;;
    badge) printf '{"schemaVersion":1,"label":"health","message":"A"}\n' > "$destination" ;;
    *) printf 'unexpected format: %s\n' "$selected" >&2; exit 64 ;;
  esac
}

[ -n "$output" ] && write_output "$format" "$output"
for emit in "${emits[@]}"; do
  write_output "${emit%%=*}" "${emit#*=}"
done

if [ "$format" = "markdown" ]; then
  exit "${FAKE_GATE_CODE:-0}"
fi
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn run_health_step(supports_emit: bool) -> (usize, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fake_codelens(&bin.join("codelens"));
    let log = dir.path().join("calls.log");
    let github_output = dir.path().join("github-output");
    let github_env = dir.path().join("github-env");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(health_step_script())
        .current_dir(dir.path())
        .env("PATH", path)
        .env("FAKE_LOG", &log)
        .env("FAKE_SUPPORTS_EMIT", if supports_emit { "1" } else { "0" })
        .env("FAKE_GATE_CODE", "7")
        .env("INPUT_PATH", ".")
        .env("INPUT_HEALTH_MODEL", "")
        .env("INPUT_FAIL_UNDER", "")
        .env("INPUT_BASELINE", "")
        .env("INPUT_FAIL_ON_REGRESSION", "false")
        .env("INPUT_SUMMARY", "false")
        .env("GITHUB_OUTPUT", &github_output)
        .env("GITHUB_ENV", &github_env)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "action script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let calls = fs::read_to_string(log).unwrap();
    let analysis_calls = calls
        .lines()
        .filter(|line| !line.contains("--help"))
        .count();
    assert!(dir.path().join("codelens-health.md").exists());
    assert!(dir.path().join("codelens-health.json").exists());
    assert!(dir.path().join("codelens-badge.json").exists());
    (
        analysis_calls,
        fs::read_to_string(github_output).unwrap(),
        fs::read_to_string(github_env).unwrap(),
    )
}

#[test]
fn action_uses_one_scan_when_emit_is_supported() {
    let (calls, outputs, environment) = run_health_step(true);
    assert_eq!(calls, 1);
    assert!(outputs.contains("score=91.5"));
    assert!(outputs.contains("grade=A"));
    assert!(outputs.contains("gate=failed"));
    assert!(
        environment.contains("GATE_CODE=7"),
        "unexpected environment output: {environment:?}"
    );
}

#[test]
fn action_falls_back_to_three_scans_for_older_releases() {
    let (calls, outputs, environment) = run_health_step(false);
    assert_eq!(calls, 3);
    assert!(outputs.contains("score=91.5"));
    assert!(outputs.contains("grade=A"));
    assert!(outputs.contains("gate=failed"));
    assert!(
        environment.contains("GATE_CODE=7"),
        "unexpected environment output: {environment:?}"
    );
}

#[test]
fn fail_on_regression_requires_a_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fake_codelens(&bin.join("codelens"));
    let log = dir.path().join("calls.log");
    let github_output = dir.path().join("github-output");
    let github_env = dir.path().join("github-env");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(health_step_script())
        .current_dir(dir.path())
        .env("PATH", path)
        .env("FAKE_LOG", &log)
        .env("FAKE_SUPPORTS_EMIT", "1")
        .env("INPUT_PATH", ".")
        .env("INPUT_HEALTH_MODEL", "")
        .env("INPUT_FAIL_UNDER", "")
        .env("INPUT_BASELINE", "")
        .env("INPUT_FAIL_ON_REGRESSION", "true")
        .env("INPUT_SUMMARY", "false")
        .env("GITHUB_OUTPUT", &github_output)
        .env("GITHUB_ENV", &github_env)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires the baseline input"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !log.exists(),
        "validation must happen before invoking codelens"
    );
}
