//! `repoctx hook doctor` drift/tamper + repair (2307c32).

use std::path::Path;

use assert_cmd::Command;

fn init(repo: &Path, home: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .args(["init", "--yes", "--rtk", "off"])
        .assert()
        .success();
}

fn doctor(repo: &Path, home: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .args(["hook", "doctor"])
        .args(extra)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn healthy_after_init() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    init(repo.path(), home.path());
    let (code, out) = doctor(repo.path(), home.path(), &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("healthy"));
}

#[test]
fn detects_script_drift_and_repairs() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    init(repo.path(), home.path());

    // Tamper with the committed script body.
    let script = repo.path().join(".repoctx/hook.sh");
    let mut body = std::fs::read_to_string(&script).unwrap();
    body.push_str("\necho HACKED\n");
    std::fs::write(&script, body).unwrap();

    let (code, out) = doctor(repo.path(), home.path(), &[]);
    assert_eq!(code, 1, "drift must exit 1");
    assert!(out.contains("drifted"), "{out}");

    // --fix repairs → healthy.
    let (code, out) = doctor(repo.path(), home.path(), &["--fix"]);
    assert_eq!(code, 0, "{out}");
    assert!(!std::fs::read_to_string(&script).unwrap().contains("HACKED"));

    let (code, _) = doctor(repo.path(), home.path(), &[]);
    assert_eq!(code, 0);
}

#[test]
fn detects_missing_settings_entry() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    init(repo.path(), home.path());
    // Blow away the settings entry.
    std::fs::write(repo.path().join(".claude/settings.json"), "{}").unwrap();
    let (code, out) = doctor(repo.path(), home.path(), &[]);
    assert_eq!(code, 1);
    assert!(out.contains("settings.json"), "{out}");
}

#[test]
fn reports_foreign_hook() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    init(repo.path(), home.path());
    // Inject a foreign project-local hook.
    std::fs::write(
        repo.path().join(".claude/settings.local.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"sneaky-tool"}]}]}}"#,
    )
    .unwrap();
    let (code, out) = doctor(repo.path(), home.path(), &[]);
    assert_eq!(code, 1);
    assert!(out.contains("sneaky-tool"), "{out}");
}
