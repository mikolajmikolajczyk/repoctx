//! End-to-end `callers`/`callees` CLI tests (epic af42572 / ADR-0010).

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
        "fn main() {\n    helper();\n    external_thing();\n}\n\nfn helper() {\n    leaf();\n}\n\nfn leaf() {}\n",
    )
    .unwrap();
    tmp
}

fn index(root: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
}

fn json(root: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.args(["--repo", root.to_str().unwrap(), "--json"]);
    cmd.args(args);
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn callers_of_helper_is_main() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["callers", "helper"]);
    assert_eq!(v["count"].as_u64().unwrap(), 1, "{v}");
    let edge = &v["items"][0];
    assert_eq!(edge["caller"]["name"], "main");
    assert_eq!(edge["callee_name"], "helper");
    assert_eq!(edge["resolution"], "syntactic");
}

#[test]
fn callees_of_main_includes_unresolved_external() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["callees", "main"]);
    let items = v["items"].as_array().unwrap();
    // helper (resolved) + external_thing (unresolved).
    assert!(items
        .iter()
        .any(|e| e["callee_name"] == "helper" && e["callee"].is_object()));
    let ext = items
        .iter()
        .find(|e| e["callee_name"] == "external_thing")
        .expect("external_thing edge");
    assert!(
        ext["callee"].is_null() || ext.get("callee").is_none(),
        "external is unresolved: {ext}"
    );
    // Advisory must fire (unresolved present).
    assert!(
        v["advisory"].as_str().unwrap_or("").contains("unresolved"),
        "{v}"
    );
}

#[test]
fn callees_resolved_only_drops_external() {
    let tmp = fixture();
    index(tmp.path());
    // main calls helper (resolved) + external_thing (unresolved). --resolved-only
    // keeps only the resolved edge.
    let v = json(tmp.path(), &["callees", "main", "--resolved-only"]);
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().all(|e| e["callee"].is_object()), "{v}");
    assert!(items.iter().any(|e| e["callee_name"] == "helper"));
    assert!(!items.iter().any(|e| e["callee_name"] == "external_thing"));
}

#[test]
fn callers_empty_has_advisory() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["callers", "main"]);
    assert_eq!(v["count"].as_u64().unwrap(), 0);
    assert!(
        v["advisory"]
            .as_str()
            .unwrap_or("")
            .contains("no call edges"),
        "{v}"
    );
}

#[test]
fn limit_is_respected() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["callees", "main", "--limit", "1"]);
    assert!(v["items"].as_array().unwrap().len() <= 1);
}

#[test]
fn callgraph_down_reaches_transitive_leaf() {
    let tmp = fixture();
    index(tmp.path());
    // main -> helper -> leaf : leaf is reachable at depth 2 downward.
    let v = json(
        tmp.path(),
        &["callgraph", "main", "--depth", "2", "--direction", "down"],
    );
    let items = v["items"].as_array().unwrap();
    assert!(
        items.iter().any(|g| g["depth"] == 2
            && g["caller"]["name"] == "helper"
            && g["callee_name"] == "leaf"),
        "{v}"
    );
    // depth-1 direct edge present too.
    assert!(items
        .iter()
        .any(|g| g["depth"] == 1 && g["callee_name"] == "helper"));
}

#[test]
fn callgraph_up_reaches_transitive_root() {
    let tmp = fixture();
    index(tmp.path());
    // leaf <- helper <- main : main reachable at depth 2 upward.
    let v = json(
        tmp.path(),
        &["callgraph", "leaf", "--depth", "2", "--direction", "up"],
    );
    let items = v["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|g| g["depth"] == 2 && g["caller"]["name"] == "main"),
        "{v}"
    );
}

#[test]
fn callgraph_depth_one_is_direct_only() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(
        tmp.path(),
        &["callgraph", "main", "--depth", "1", "--direction", "down"],
    );
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().all(|g| g["depth"] == 1), "{v}");
    // leaf is depth 2, must be absent.
    assert!(!items.iter().any(|g| g["callee_name"] == "leaf"));
}

#[test]
fn callgraph_invalid_direction_errors() {
    let tmp = fixture();
    index(tmp.path());
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "callgraph",
            "main",
            "--direction",
            "sideways",
        ])
        .assert()
        .failure();
}
