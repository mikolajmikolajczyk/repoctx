//! End-to-end `deps`/`rdeps` CLI tests (import graph, epic #4 / ADR-0011).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/ui.ts"),
        "import { saveFile } from \"@adapters/storage-idb\";\nimport { fmt } from \"./util\";\n",
    )
    .unwrap();
    fs::write(
        root.join("src/svc.ts"),
        "import type { Manifest } from \"@adapters/storage-idb\";\n",
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
fn deps_lists_module_specifiers() {
    let tmp = fixture();
    index(tmp.path());
    let ui = tmp.path().join("src/ui.ts");
    let v = json(tmp.path(), &["deps", ui.to_str().unwrap()]);
    assert_eq!(v["count"].as_u64().unwrap(), 2, "{v}");
    let mods: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["module"].as_str().unwrap())
        .collect();
    assert_eq!(mods, ["@adapters/storage-idb", "./util"]);
}

#[test]
fn rdeps_finds_importers_by_substring() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["rdeps", "storage-idb"]);
    assert_eq!(v["count"].as_u64().unwrap(), 2, "{v}");
    let files: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap())
        .collect();
    assert!(files.contains(&"src/ui.ts"));
    assert!(files.contains(&"src/svc.ts"));
}

#[test]
fn rdeps_empty_has_advisory() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["rdeps", "nonexistent-module"]);
    assert_eq!(v["count"].as_u64().unwrap(), 0);
    assert!(v["advisory"].is_string(), "empty rdeps carries an advisory");
}

#[test]
fn boundary_reports_and_gates() {
    let tmp = fixture();
    let root = tmp.path();
    // fixture: src/ui.ts imports @adapters/storage-idb + ./util; src/svc.ts
    // imports @adapters/storage-idb.
    let v = json(root, &["boundary", "--from", "src/ui", "--to", "@adapters"]);
    assert_eq!(v["count"].as_u64().unwrap(), 1, "{v}");
    assert_eq!(v["items"][0]["file"], "src/ui.ts");

    // --forbid gates: crossing -> exit 1.
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "boundary",
            "--from",
            "src/ui",
            "--to",
            "@adapters",
            "--forbid",
        ])
        .assert()
        .failure();

    // Clean boundary -> exit 0 + advisory.
    let clean = json(root, &["boundary", "--from", "src/ui", "--to", "@nope"]);
    assert_eq!(clean["count"].as_u64().unwrap(), 0);
    assert!(clean["advisory"].is_string());
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            root.to_str().unwrap(),
            "boundary",
            "--from",
            "src/ui",
            "--to",
            "@nope",
            "--forbid",
        ])
        .assert()
        .success();
}

#[test]
fn boundary_zero_with_aliased_imports_warns_not_clean() {
    // fixture: src/ui.ts imports @adapters/storage-idb (alias) + ./util.
    let tmp = fixture();
    let root = tmp.path();
    // No RELATIVE crossing for `--to @adapters` (alias isn't relative-resolved),
    // but aliased imports exist → advisory must say NOT clean.
    let v = json(root, &["boundary", "--from", "src", "--to", "src/adapters"]);
    assert_eq!(v["count"].as_u64().unwrap(), 0);
    let adv = v["advisory"].as_str().unwrap();
    assert!(
        adv.contains("NOT checked") && adv.contains("NOT a clean bill"),
        "{adv}"
    );
}

#[test]
fn deps_outside_repo_errors() {
    let tmp = fixture();
    index(tmp.path());
    Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--json",
            "deps",
            "/etc/hosts",
        ])
        .assert()
        .failure();
}
