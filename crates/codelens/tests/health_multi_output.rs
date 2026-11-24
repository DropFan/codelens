use std::fs;
use std::process::Command;

use serde_json::Value;

#[test]
fn health_writes_markdown_json_and_badge_from_one_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("lib.rs"),
        "pub fn health(value: usize) -> usize {\n    if value > 0 { value } else { 1 }\n}\n",
    )
    .unwrap();

    let markdown = dir.path().join("health.md");
    let json = dir.path().join("health.json");
    let badge = dir.path().join("badge.json");
    let output = Command::new(env!("CARGO_BIN_EXE_codelens"))
        .arg("health")
        .arg(&source)
        .arg("--no-config")
        .arg("-f")
        .arg("markdown")
        .arg("-O")
        .arg(&markdown)
        .arg("--emit")
        .arg(format!("json={}", json.display()))
        .arg("--emit")
        .arg(format!("badge={}", badge.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "health command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let markdown = fs::read_to_string(markdown).unwrap();
    let json: Value = serde_json::from_slice(&fs::read(json).unwrap()).unwrap();
    let badge: Value = serde_json::from_slice(&fs::read(badge).unwrap()).unwrap();
    assert!(markdown.contains("Code Health Report"));
    assert!(json["score"].is_number());
    assert!(json["grade"].is_string());
    assert_eq!(badge["schemaVersion"], 1);
    assert_eq!(badge["label"], "code health");
}
