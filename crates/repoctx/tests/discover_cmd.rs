//! End-to-end hook telemetry → `repoctx discover` (issue #7).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("a.rs"), "fn saveFile() {}\nfn parseThing() {}\n").unwrap();
    // Index so .repoctx/index.db exists (telemetry is gated on it).
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
    tmp
}

fn hook(root: &Path, command: &str) {
    let stdin = serde_json::json!({ "tool_input": { "command": command } }).to_string();
    let _ = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "hook",
            "claude",
            "--rtk-chain=0",
        ])
        .write_stdin(stdin)
        .assert();
}

fn discover(root: &Path) -> Value {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "discover"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn telemetry_records_and_discover_reports() {
    let tmp = fixture();
    let root = tmp.path();

    hook(root, "rg saveFile"); // bare-ident -> rewritten
    hook(root, "rg parseThing"); // bare-ident -> rewritten
    hook(root, "grep -rn fooBar ."); // flagged-nav-ident -> passthrough
    hook(root, "cargo build"); // not grep-family -> no event

    let v = discover(root);
    assert_eq!(v["events"].as_u64().unwrap(), 3, "{v}");
    let idioms = v["idioms"].as_array().unwrap();

    let bare = idioms.iter().find(|r| r["idiom"] == "bare-ident").unwrap();
    assert_eq!(bare["rewritten"], 2);
    assert_eq!(bare["passthrough"], 0);
    assert_eq!(bare["rewritten_pct"], 100);

    let flagged = idioms
        .iter()
        .find(|r| r["idiom"] == "flagged-nav-ident")
        .unwrap();
    assert_eq!(flagged["passthrough"], 1);
    assert_eq!(flagged["rewritten_pct"], 0);
}

#[test]
fn telemetry_opt_out_records_nothing() {
    let tmp = fixture();
    let root = tmp.path();
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "config",
            "set",
            "hook.telemetry",
            "false",
        ])
        .assert()
        .success();

    hook(root, "rg saveFile");
    let v = discover(root);
    assert_eq!(v["events"].as_u64().unwrap(), 0);
}

#[test]
fn samples_captured_only_when_enabled() {
    let tmp = fixture();
    let root = tmp.path();

    // Default off: no samples even after a command.
    hook(root, "rg some-thing");
    let off = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "--json",
            "discover",
            "--samples",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let off: Value = serde_json::from_slice(&off).unwrap();
    assert_eq!(off["count"].as_u64().unwrap(), 0);

    // Enable, run a command, sample is captured with its body.
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "config",
            "set",
            "hook.telemetry_samples",
            "true",
        ])
        .assert()
        .success();
    hook(root, "rg widget-panel");
    let on = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "--json",
            "discover",
            "--samples",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let on: Value = serde_json::from_slice(&on).unwrap();
    let samples = on["samples"].as_array().unwrap();
    assert!(samples.iter().any(|s| s["command"] == "rg widget-panel"));
}

#[test]
fn discover_empty_repo_has_advisory() {
    let tmp = tempfile::tempdir().unwrap();
    let v = discover(tmp.path());
    assert_eq!(v["events"].as_u64().unwrap(), 0);
    assert!(v["advisory"].is_string());
}
