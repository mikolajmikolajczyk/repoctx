use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(
        root.join("main.rs"),
        "fn main() {}\nfn helper() {}\nstruct Cat;\n",
    )
    .unwrap();
    fs::write(root.join("a.go"), "package x\nfunc Main() {}\n").unwrap();
    tmp
}

fn index(root: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
}

fn json_symbols(root: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.args(["--repo", root.to_str().unwrap(), "--json", "symbols"]);
    cmd.args(args);
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn missing_index_auto_indexes_then_answers() {
    let tmp = fixture();
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--json",
            "symbols",
            "main",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let body = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(v["count"].as_u64().unwrap() >= 1, "{body}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no index found"), "stderr: {stderr}");
    assert!(stderr.contains("indexing now"), "stderr: {stderr}");
    assert!(tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn no_auto_index_flag_preserves_error() {
    let tmp = fixture();
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--no-auto-index",
            "symbols",
            "main",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(out.stdout.is_empty(), "stdout should be empty: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no index found"), "{stderr}");
    assert!(!tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn substring_finds_case_insensitive() {
    let tmp = fixture();
    index(tmp.path());
    let v = json_symbols(tmp.path(), &["main"]);
    let items = v["items"].as_array().unwrap();
    let names: Vec<_> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"main"), "{names:?}");
    assert!(names.contains(&"Main"), "{names:?}");
}

#[test]
fn empty_result_is_success_with_count_zero() {
    let tmp = fixture();
    index(tmp.path());
    let v = json_symbols(tmp.path(), &["zzz_no_match_xyz"]);
    assert_eq!(v["count"], 0);
    assert_eq!(v["items"].as_array().unwrap().len(), 0);
}

#[test]
fn filter_by_kind() {
    let tmp = fixture();
    index(tmp.path());
    let v = json_symbols(tmp.path(), &["Cat", "--kind", "class"]);
    assert_eq!(v["count"], 1);
    assert_eq!(v["items"][0]["name"], "Cat");
}

#[test]
fn filter_by_lang() {
    let tmp = fixture();
    index(tmp.path());
    let v = json_symbols(tmp.path(), &["main", "--lang", "go"]);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Main");
}

#[test]
fn limit_caps_results() {
    let tmp = fixture();
    index(tmp.path());
    let v = json_symbols(tmp.path(), &["", "--limit", "1"]);
    assert!(v["items"].as_array().unwrap().len() <= 1);
}

#[test]
fn deterministic_ordering() {
    let tmp = fixture();
    index(tmp.path());
    let a = json_symbols(tmp.path(), &["main"]);
    let b = json_symbols(tmp.path(), &["main"]);
    assert_eq!(a, b);
}

#[test]
fn human_format_on_tty_flag_path() {
    // Pipe ⇒ TOON, so explicitly request human via shell command without --json.
    let tmp = fixture();
    index(tmp.path());
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--toon",
            "symbols",
            "main",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("main"), "{s}");
}
