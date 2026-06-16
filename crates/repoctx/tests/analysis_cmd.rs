//! End-to-end `deadcode` / `impact` / `cycles` (issue #3).

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
        root.join("a.rs"),
        "fn main() { used(); a(); }\n\
         fn used() { helper(); }\n\
         fn helper() { used(); }\n\
         fn a() { b(); }\n\
         fn b() { a(); }\n\
         fn orphan() {}\n",
    )
    .unwrap();
    tmp
}

fn json(root: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.args(["--repo", root.to_str().unwrap(), "--json"])
        .args(args);
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn deadcode_lists_uncalled_excluding_main() {
    let tmp = fixture();
    let v = json(tmp.path(), &["deadcode"]);
    let names: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"orphan"), "{names:?}");
    assert!(!names.contains(&"main"), "main excluded as entry point");
    assert!(!names.contains(&"used"));
    assert!(v["advisory"].is_string());
}

#[test]
fn impact_shows_transitive_callers() {
    let tmp = fixture();
    let v = json(tmp.path(), &["impact", "used", "--depth", "3"]);
    // every edge walks the caller (up) direction.
    assert!(v["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["direction"] == "up"));
    assert!(v["count"].as_u64().unwrap() >= 1);
}

#[test]
fn cycles_detects_mutual_recursion() {
    let tmp = fixture();
    let v = json(tmp.path(), &["cycles"]);
    // a<->b and used<->helper.
    assert_eq!(v["count"].as_u64().unwrap(), 2, "{v}");
    let chains: Vec<Vec<String>> = serde_json::from_value(v["cycles"].clone()).unwrap();
    assert!(chains
        .iter()
        .any(|c| c.contains(&"a".to_string()) && c.contains(&"b".to_string())));
}
