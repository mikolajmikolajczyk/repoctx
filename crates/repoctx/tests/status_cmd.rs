use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(root.join("b.rs"), "fn b() {}\n").unwrap();
    fs::write(root.join("c.go"), "package x\nfunc C() {}\n").unwrap();
    tmp
}

fn index(root: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
}

fn status_json(root: &Path, extra: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.args(["--repo", root.to_str().unwrap(), "--json", "status"]);
    cmd.args(extra);
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn missing_index_auto_indexes_then_runs_status() {
    let tmp = fixture();
    let v = status_json(tmp.path(), &[]);
    assert!(v["files"].as_u64().unwrap() >= 3);
    assert!(tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn fresh_index_has_zero_staleness_and_correct_counts() {
    let tmp = fixture();
    index(tmp.path());
    let v = status_json(tmp.path(), &[]);
    assert_eq!(v["files"], 3);
    assert!(v["symbols"].as_u64().unwrap() >= 3);
    assert_eq!(v["schema_version"], 6);
    assert!(v["db_size_bytes"].as_u64().unwrap() > 0);
    let st = &v["staleness"];
    assert_eq!(st["changed"], 0);
    assert_eq!(st["new"], 0);
    assert_eq!(st["deleted"], 0);
    let langs: Vec<_> = v["per_language"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["language"].as_str().unwrap())
        .collect();
    assert!(langs.contains(&"rust"));
    assert!(langs.contains(&"go"));
}

#[test]
fn edit_new_delete_reflected_in_staleness() {
    let tmp = fixture();
    index(tmp.path());
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(tmp.path().join("a.rs"), "fn a() { let _ = 1; }\n").unwrap();
    fs::write(tmp.path().join("d.rs"), "fn d() {}\n").unwrap();
    fs::remove_file(tmp.path().join("b.rs")).unwrap();
    let v = status_json(tmp.path(), &[]);
    assert_eq!(v["staleness"]["changed"], 1, "{v}");
    assert_eq!(v["staleness"]["new"], 1, "{v}");
    assert_eq!(v["staleness"]["deleted"], 1, "{v}");
}

#[test]
fn fast_skips_staleness() {
    let tmp = fixture();
    index(tmp.path());
    let v = status_json(tmp.path(), &["--fast"]);
    assert!(v.get("staleness").is_none() || v["staleness"].is_null());
}
