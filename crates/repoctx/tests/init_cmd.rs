//! End-to-end coverage for `repoctx init` — the SessionStart priming install.
//!
//! Drives the real CLI into a tempdir and asserts it installs the Claude
//! guidance + a SessionStart hook running `repoctx prime`, idempotently, and
//! that `--uninstall` removes the hook again.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run(repo: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("repoctx")
        .unwrap()
        .arg("--repo")
        .arg(repo)
        .args(args)
        .assert()
}

fn settings(repo: &Path) -> Value {
    let p = repo.join(".claude/settings.json");
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn prime_hooks(v: &Value) -> usize {
    v.get("hooks")
        .and_then(|h| h.get("SessionStart"))
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|e| {
                    e.get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|hs| {
                            hs.iter().any(|h| {
                                h.get("command")
                                    .and_then(|c| c.as_str())
                                    .map(|s| s.contains("session-start.sh"))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn init_installs_session_start_script_and_guidance() {
    let repo = TempDir::new().unwrap();
    run(repo.path(), &["init", "--yes"]).success();

    // SessionStart hook points at the script.
    assert_eq!(prime_hooks(&settings(repo.path())), 1);
    // The script exists with the managed prime block.
    let script = repo.path().join(".claude/hooks/session-start.sh");
    assert!(script.exists());
    let body = fs::read_to_string(&script).unwrap();
    assert!(body.contains("repoctx prime"));
    assert!(body.contains("your session-start context below"));
    // Guidance skill written.
    assert!(repo
        .path()
        .join(".claude/skills/repoctx/SKILL.md")
        .exists());
}

#[test]
fn init_is_idempotent() {
    let repo = TempDir::new().unwrap();
    run(repo.path(), &["init", "--yes"]).success();
    run(repo.path(), &["init", "--yes", "--force"]).success();
    assert_eq!(prime_hooks(&settings(repo.path())), 1, "no duplicate hook");
}

#[test]
fn init_dry_run_writes_nothing() {
    let repo = TempDir::new().unwrap();
    run(repo.path(), &["init", "--yes", "--dry-run"]).success();
    assert!(!repo.path().join(".claude/settings.json").exists());
}

#[test]
fn uninstall_removes_session_start_hook() {
    let repo = TempDir::new().unwrap();
    run(repo.path(), &["init", "--yes"]).success();
    assert_eq!(prime_hooks(&settings(repo.path())), 1);
    run(repo.path(), &["init", "--uninstall"]).success();
    assert_eq!(prime_hooks(&settings(repo.path())), 0);
}
