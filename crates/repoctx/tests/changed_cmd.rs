//! End-to-end `repoctx changed` (issue #6). Needs a real git repo + commit.

use std::fs;
use std::path::Path;
use std::process::Command as Proc;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let ok = Proc::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Repo with a committed call chain leaf←helper←caller←top.
fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    fs::write(
        root.join("a.rs"),
        "fn helper() { leaf(); }\nfn caller() { helper(); }\nfn top() { caller(); }\nfn leaf() {}\n",
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qn", "-m", "init"]);
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
    tmp
}

fn changed(root: &Path) -> Value {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "changed"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn changed_symbol_and_blast_radius() {
    let tmp = fixture();
    let root = tmp.path();
    // Edit helper's body (line 1).
    fs::write(
        root.join("a.rs"),
        "fn helper() { leaf(); leaf(); }\nfn caller() { helper(); }\nfn top() { caller(); }\nfn leaf() {}\n",
    )
    .unwrap();

    let v = changed(root);
    assert_eq!(v["files_changed"].as_u64().unwrap(), 1, "{v}");

    let changed_names: Vec<&str> = v["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(changed_names, ["helper"]);

    // blast radius = transitive callers: caller (d1), top (d2).
    let imp: Vec<(&str, u64)> = v["impacted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["name"].as_str().unwrap(), i["depth"].as_u64().unwrap()))
        .collect();
    assert!(imp.contains(&("caller", 1)), "{imp:?}");
    assert!(imp.contains(&("top", 2)), "{imp:?}");
    // leaf is a callee of helper, not a caller -> not in blast radius.
    assert!(!imp.iter().any(|(n, _)| *n == "leaf"));
}

#[test]
fn no_changes_is_clean() {
    let tmp = fixture();
    let v = changed(tmp.path());
    assert_eq!(v["files_changed"].as_u64().unwrap(), 0);
    assert!(v["changed"].as_array().unwrap().is_empty());
    assert!(v["advisory"].as_str().unwrap().contains("no changes"));
}
