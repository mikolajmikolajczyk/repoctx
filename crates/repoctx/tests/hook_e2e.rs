//! End-to-end coverage for `repoctx hook install` / `list`.
//!
//! Integration content is embedded in the binary — there is no network
//! path and no cache to seed. These tests drive the CLI against the real
//! embedded `claude` / `codex` / `opencode` manifests into a tempdir
//! target, and pass `--repo <target>` so any config writes stay isolated.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// `repoctx --json --repo <target> hook <extra...> --dir <target>`.
fn run(target: &Path, extra_args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("RUST_REPOCTX_NO_RECORD", "1")
        .arg("--repo")
        .arg(target)
        .args(["--json", "hook"])
        .args(extra_args)
        .arg("--dir")
        .arg(target);
    cmd.assert()
}

fn json(out: &[u8]) -> Value {
    serde_json::from_slice(out).unwrap()
}

fn target() -> TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn clean_install_writes_files() {
    let t = target();
    let assert = run(t.path(), &["install", "codex"]).success();
    let v: Value = json(&assert.get_output().stdout);
    assert_eq!(v["agent"], "codex");
    assert_eq!(v["dry_run"], false);
    let written = v["written"].as_array().unwrap();
    // codex: SKILL.md (write) + AGENTS.md (merge-section).
    assert_eq!(written.len(), 2);
    assert_eq!(written[0]["action"], "created");
    let skill = t.path().join(".agents/skills/repoctx/SKILL.md");
    assert!(skill.exists());
    let body = fs::read_to_string(&skill).unwrap();
    // {REPOCTX_BIN} substituted to a path containing the binary name.
    assert!(body.contains("repoctx"));
    assert!(!body.contains("{REPOCTX_BIN}"));
}

#[test]
fn dry_run_writes_nothing() {
    let t = target();
    let assert = run(t.path(), &["install", "codex", "--dry-run"]).success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "dry_run");
    assert!(!t.path().join(".agents/skills/repoctx/SKILL.md").exists());
}

#[test]
fn idempotent_reinstall_reports_skipped_identical() {
    let t = target();
    run(t.path(), &["install", "codex"]).success();
    let assert = run(t.path(), &["install", "codex"]).success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "skipped_identical");
}

#[test]
fn force_required_to_overwrite_local_edit() {
    let t = target();
    run(t.path(), &["install", "codex"]).success();
    let skill = t.path().join(".agents/skills/repoctx/SKILL.md");
    fs::write(&skill, "edited locally\n").unwrap();
    // Without --force: error.
    run(t.path(), &["install", "codex"]).failure();
    // With --force: Updated.
    let assert = run(t.path(), &["install", "codex", "--force"]).success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "updated");
}

#[test]
fn merge_section_install_then_idempotent() {
    let t = target();
    let assert = run(t.path(), &["install", "codex"]).success();
    let v = json(&assert.get_output().stdout);
    // second file is the AGENTS.md merge-section block.
    assert_eq!(v["written"][1]["action"], "created");
    let agents = fs::read_to_string(t.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("<!-- repoctx:start -->"));
    assert!(agents.contains("<!-- repoctx:end -->"));

    let assert2 = run(t.path(), &["install", "codex"]).success();
    let v2 = json(&assert2.get_output().stdout);
    assert_eq!(v2["written"][1]["action"], "skipped_identical");
}

#[test]
fn unknown_agent_errors_with_known_list() {
    let t = target();
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("RUST_REPOCTX_NO_RECORD", "1")
        .arg("--repo")
        .arg(t.path())
        .args(["hook", "install", "aider"])
        .arg("--dir")
        .arg(t.path());
    let output = cmd.assert().failure().get_output().stderr.clone();
    let s = String::from_utf8_lossy(&output);
    assert!(s.contains("unknown agent: aider"));
    assert!(s.contains("claude"));
    assert!(s.contains("codex"));
    assert!(s.contains("opencode"));
}

#[test]
fn hook_list_returns_three_agents_with_descriptions() {
    let t = target();
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("RUST_REPOCTX_NO_RECORD", "1")
        .arg("--repo")
        .arg(t.path())
        .args(["--json", "hook", "list"]);
    let output = cmd.assert().success().get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(v["count"], 3);
    let items = v["items"].as_array().unwrap();
    let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["claude", "codex", "opencode"]);
    // Descriptions come from embedded manifests — always present now.
    for i in items {
        assert!(i["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
}
