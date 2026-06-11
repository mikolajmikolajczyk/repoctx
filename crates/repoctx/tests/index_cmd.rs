use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        root.join("lib.rs"),
        "pub struct Cat;\npub fn meow() -> &'static str { \"meow\" }\n",
    )
    .unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/a.go"), "package sub\nfunc Foo() {}\n").unwrap();
    fs::write(root.join("README.md"), "# Title\n\n## Sub\n").unwrap();
    fs::write(root.join("Config.toml"), "name = \"x\"\n[pkg]\nv = \"1\"\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(root.join("ignored.rs"), "fn skipme() {}\n").unwrap();
    tmp
}

fn run_index(root: &Path) -> Value {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn fresh_index_counts_files_and_persists_db() {
    let tmp = fixture();
    let s = run_index(tmp.path());
    assert_eq!(s["unchanged"], 0);
    assert_eq!(s["removed"], 0);
    let n = s["indexed"].as_u64().unwrap();
    // main.rs, lib.rs, sub/a.go, README.md, Config.toml — ignored.rs gitignored.
    assert_eq!(n, 5, "got summary {s}");
    assert!(tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn second_run_is_no_op() {
    let tmp = fixture();
    run_index(tmp.path());
    let s = run_index(tmp.path());
    assert_eq!(s["indexed"], 0, "{s}");
    assert!(s["unchanged"].as_u64().unwrap() >= 5, "{s}");
    assert_eq!(s["removed"], 0);
}

#[test]
fn editing_one_file_reindexes_only_that_file() {
    let tmp = fixture();
    run_index(tmp.path());
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(
        tmp.path().join("main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    let s = run_index(tmp.path());
    assert_eq!(s["indexed"], 1, "{s}");
    assert_eq!(s["removed"], 0, "{s}");
}

#[test]
fn deleting_a_file_prunes() {
    let tmp = fixture();
    run_index(tmp.path());
    fs::remove_file(tmp.path().join("lib.rs")).unwrap();
    let s = run_index(tmp.path());
    assert_eq!(s["indexed"], 0, "{s}");
    assert_eq!(s["removed"], 1, "{s}");
}

#[test]
fn force_reparses_every_file() {
    let tmp = fixture();
    run_index(tmp.path());
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--json",
            "index",
            "--force",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s: Value = serde_json::from_slice(&out).unwrap();
    assert!(s["indexed"].as_u64().unwrap() >= 5, "{s}");
    assert_eq!(s["unchanged"], 0);
}

#[test]
fn oversize_file_is_skipped() {
    let tmp = fixture();
    let big = vec![b'a'; (2 * 1024 * 1024 + 1) as usize];
    fs::write(tmp.path().join("huge.rs"), &big).unwrap();
    let s = run_index(tmp.path());
    // Same 5 files — huge.rs skipped.
    assert_eq!(s["indexed"].as_u64().unwrap(), 5, "{s}");
}

#[test]
fn non_utf8_file_is_skipped() {
    let tmp = fixture();
    fs::write(tmp.path().join("bad.rs"), [0xff, 0xfe, 0xfd]).unwrap();
    let s = run_index(tmp.path());
    assert_eq!(s["indexed"].as_u64().unwrap(), 5, "{s}");
}
