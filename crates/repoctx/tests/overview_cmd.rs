//! End-to-end `repoctx overview` (issue #5).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn main() { helper(); helper(); }\nfn helper() {}\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn util() {}\n").unwrap();
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
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
fn overview_composes_the_index_and_call_graph() {
    let tmp = fixture();
    let v = json(tmp.path(), &["overview"]);

    assert!(v["files"].as_u64().unwrap() >= 2, "{v}");
    assert!(v["symbols"].as_u64().unwrap() >= 3);

    // languages include rust.
    let langs: Vec<&str> = v["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["lang"].as_str().unwrap())
        .collect();
    assert!(langs.contains(&"rust"));

    // module `src` aggregated.
    assert!(v["modules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["dir"] == "src"));

    // entry point main.
    assert!(v["entry_points"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "main"));

    // hotspot helper (called twice).
    let helper = v["hotspots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["name"] == "helper");
    assert!(helper.is_some(), "helper should be a hotspot: {v}");
    assert_eq!(helper.unwrap()["callers"], 2);

    // public API surface (#10): `pub fn util` in src/lib.rs is exported; the
    // private `helper`/`main` are not.
    let pub_src = v["public_api"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["dir"] == "src")
        .expect("src should have a public-API entry");
    let syms: Vec<&str> = pub_src["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert!(syms.contains(&"util:function"), "util exported: {pub_src}");
    assert!(!syms.iter().any(|s| s.starts_with("helper")), "helper is private");
}
