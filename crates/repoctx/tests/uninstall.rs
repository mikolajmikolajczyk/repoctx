//! `repoctx init --uninstall` (ec698bb).

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn cli(repo: &Path, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .args(args)
        .assert()
}

fn bash_count(settings: &Path) -> usize {
    let Ok(text) = std::fs::read_to_string(settings) else {
        return 0;
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    v["hooks"]["PreToolUse"]
        .as_array()
        .map(|a| a.iter().filter(|e| e["matcher"] == "Bash").count())
        .unwrap_or(0)
}

#[test]
fn uninstall_removes_entry_and_script() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    cli(repo.path(), home.path(), &["init", "--yes", "--rtk", "off"]).success();
    assert!(repo.path().join(".repoctx/hook.sh").exists());

    cli(repo.path(), home.path(), &["init", "--uninstall"]).success();
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
    assert_eq!(bash_count(&repo.path().join(".claude/settings.json")), 0);
}

#[test]
fn uninstall_preserves_foreign_entries() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    cli(repo.path(), home.path(), &["init", "--yes", "--rtk", "off"]).success();
    // Add a foreign hook in the project-local scope (different file, kept).
    std::fs::write(
        repo.path().join(".claude/settings.local.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"keep-me"}]}]}}"#,
    )
    .unwrap();

    cli(repo.path(), home.path(), &["init", "--uninstall"]).success();
    // Our entry (settings.json) gone; the foreign one (settings.local.json) stays.
    assert_eq!(bash_count(&repo.path().join(".claude/settings.json")), 0);
    let local = std::fs::read_to_string(repo.path().join(".claude/settings.local.json")).unwrap();
    assert!(local.contains("keep-me"));
}

#[test]
fn uninstall_refuses_drifted_script_without_force() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    cli(repo.path(), home.path(), &["init", "--yes", "--rtk", "off"]).success();
    // Replace the script with a non-repoctx one (no marker).
    std::fs::write(
        repo.path().join(".repoctx/hook.sh"),
        "#!/bin/sh\necho mine\n",
    )
    .unwrap();

    cli(repo.path(), home.path(), &["init", "--uninstall"]).failure();
    assert!(
        repo.path().join(".repoctx/hook.sh").exists(),
        "kept on refusal"
    );

    // --force removes it.
    cli(
        repo.path(),
        home.path(),
        &["init", "--uninstall", "--force"],
    )
    .success();
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
}

#[test]
fn dry_run_changes_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    cli(repo.path(), home.path(), &["init", "--yes", "--rtk", "off"]).success();
    cli(
        repo.path(),
        home.path(),
        &["init", "--uninstall", "--dry-run"],
    )
    .success();
    assert!(repo.path().join(".repoctx/hook.sh").exists());
    assert_eq!(bash_count(&repo.path().join(".claude/settings.json")), 1);
}

#[test]
fn global_restore_backup() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    // Pre-existing global rtk hook → init -g backs it up while taking over.
    let gclaude = home.path().join(".claude");
    std::fs::create_dir_all(&gclaude).unwrap();
    std::fs::write(
        gclaude.join("settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#,
    )
    .unwrap();
    cli(repo.path(), home.path(), &["init", "-g", "--yes"]).success();

    // Restore the backup → rtk entry is back.
    cli(
        repo.path(),
        home.path(),
        &["init", "--uninstall", "-g", "--restore-backup"],
    )
    .success();
    let restored = std::fs::read_to_string(gclaude.join("settings.json")).unwrap();
    assert!(
        restored.contains("rtk hook claude"),
        "backup restored: {restored}"
    );
}
