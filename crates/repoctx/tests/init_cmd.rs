//! End-to-end coverage for `repoctx init` (4b2af2a).
//!
//! HOME is pointed at a throwaway tempdir so the user-global conflict
//! scan stays hermetic, and `--rtk on|off` is passed explicitly so
//! RTK_CHAIN is deterministic regardless of whether rtk is on PATH.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run(repo: &Path, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .arg("init")
        .args(args)
        .assert()
}

fn bash_command(settings_path: &Path) -> String {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(settings_path).unwrap()).unwrap();
    v["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["matcher"] == "Bash")
        .expect("Bash matcher entry")["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn dry_run_writes_nothing() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(
        repo.path(),
        home.path(),
        &["--yes", "--rtk", "off", "--dry-run"],
    )
    .success();
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
    assert!(!repo.path().join(".claude/settings.json").exists());
}

#[test]
fn local_install_writes_script_settings_gitattributes_and_guidance() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();

    let script = repo.path().join(".repoctx/hook.sh");
    let body = std::fs::read_to_string(&script).unwrap();
    assert!(body.contains("# repoctx-hook-version: 1"));
    assert!(body.contains("RTK_CHAIN=1"));
    assert!(body.contains(r#"exec "$REPOCTX" hook claude --rtk-chain="$RTK_CHAIN""#));
    // (executable bit is set best-effort via `chmod`; not asserted here to
    // keep this suite free of OS-specific permission APIs — the e2e matrix
    // 0a338d7 verifies executability behaviorally.)

    assert_eq!(
        bash_command(&repo.path().join(".claude/settings.json")),
        ".repoctx/hook.sh"
    );

    let gitattrs = std::fs::read_to_string(repo.path().join(".gitattributes")).unwrap();
    assert!(gitattrs.contains("*.sh text eol=lf"));

    assert!(repo.path().join(".claude/skills/repoctx/SKILL.md").exists());
    let claude_md = std::fs::read_to_string(repo.path().join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("<!-- repoctx:start -->"));
}

#[test]
fn rtk_off_bakes_zero() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "off"]).success();
    let body = std::fs::read_to_string(repo.path().join(".repoctx/hook.sh")).unwrap();
    assert!(body.contains("RTK_CHAIN=0"));
}

#[test]
fn global_install_writes_to_home_with_absolute_path() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["-g", "--yes", "--rtk", "off"]).success();

    let script = home.path().join(".claude/repoctx-hook.sh");
    assert!(script.exists(), "global script written under HOME/.claude");

    let cmd = bash_command(&home.path().join(".claude/settings.json"));
    assert_eq!(
        cmd,
        script.display().to_string(),
        "global entry is absolute"
    );

    // No project files for a global install.
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
    assert!(!repo.path().join("CLAUDE.md").exists());
}

#[test]
fn idempotent_second_run() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();
    // Settings still has exactly one Bash entry pointing at the script.
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(repo.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let bash_entries = v["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["matcher"] == "Bash")
        .count();
    assert_eq!(bash_entries, 1);
}

#[test]
fn unknown_agent_rejected() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let out = run(repo.path(), home.path(), &["--yes", "--agent", "aider"])
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&out).contains("unsupported agent 'aider'"));
}
