//! End-to-end `modules` / `import-cycles` (issue #4, petgraph).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture(cyclic: bool) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src/ui")).unwrap();
    fs::create_dir_all(root.join("src/core")).unwrap();
    fs::write(
        root.join("src/ui/a.ts"),
        "import { b } from \"./b\";\nimport { x } from \"@adapters/db\";\n",
    )
    .unwrap();
    if cyclic {
        fs::write(root.join("src/ui/b.ts"), "import { a } from \"./a\";\n").unwrap();
    } else {
        fs::write(root.join("src/ui/b.ts"), "export const b = 1;\n").unwrap();
    }
    fs::write(
        root.join("src/ui/c.ts"),
        "import { helper } from \"../core/util\";\n",
    )
    .unwrap();
    fs::write(root.join("src/core/util.ts"), "export const helper = 1;\n").unwrap();
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
fn modules_resolves_relative_excludes_alias() {
    let tmp = fixture(false);
    let v = json(tmp.path(), &["modules"]);
    assert_eq!(v["cyclic"], false, "{v}");
    // ./b and ../core/util resolve; @adapters/db is external.
    assert_eq!(v["edges"].as_u64().unwrap(), 2);
    assert_eq!(v["external_edges"].as_u64().unwrap(), 1);
    // dependency-first order: util before c, b before a.
    let order: Vec<String> = serde_json::from_value(v["order"].clone()).unwrap();
    let pos = |p: &str| order.iter().position(|x| x == p).unwrap();
    assert!(pos("src/core/util.ts") < pos("src/ui/c.ts"));
    assert!(pos("src/ui/b.ts") < pos("src/ui/a.ts"));
}

#[test]
fn import_cycles_detects_circular() {
    let tmp = fixture(true);
    let v = json(tmp.path(), &["import-cycles"]);
    assert_eq!(v["count"].as_u64().unwrap(), 1, "{v}");
    let cycles: Vec<Vec<String>> = serde_json::from_value(v["cycles"].clone()).unwrap();
    assert!(cycles[0].contains(&"src/ui/a.ts".to_string()));
    assert!(cycles[0].contains(&"src/ui/b.ts".to_string()));
}

#[test]
fn import_cycles_clean_when_acyclic() {
    let tmp = fixture(false);
    let v = json(tmp.path(), &["import-cycles"]);
    assert_eq!(v["count"].as_u64().unwrap(), 0);
    assert!(v["advisory"].is_string());
}
