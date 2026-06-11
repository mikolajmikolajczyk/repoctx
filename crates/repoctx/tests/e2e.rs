//! End-to-end suite for the M0 surface: fixture covers every supported
//! language, edge cases (gitignored / oversized / non-UTF-8 / nested /
//! deletion), and the full index -> symbols -> status -> re-index flow.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn full_fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::create_dir(root.join("src")).unwrap();

    // One file per supported language (9 languages, 10 extensions counting tsx).
    fs::write(root.join("src/main.rs"), "fn main() {}\nstruct Cat;\n").unwrap();
    fs::write(root.join("src/lib.go"), "package x\nfunc Hello() {}\n").unwrap();
    fs::write(
        root.join("src/a.ts"),
        "export interface Speak { speak(): string }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/a.tsx"),
        "export interface View { render(): string }\n",
    )
    .unwrap();
    fs::write(root.join("src/a.js"), "class Cat {}\nfunction hello() {}\n").unwrap();
    fs::write(
        root.join("src/a.py"),
        "class Cat:\n    pass\n\ndef hello():\n    pass\n",
    )
    .unwrap();
    fs::write(
        root.join("config.json"),
        "{\"name\": \"x\", \"version\": 1}\n",
    )
    .unwrap();
    fs::write(root.join("config.yaml"), "name: x\nversion: 1\n").unwrap();
    fs::write(root.join("Config.toml"), "name = \"x\"\n[pkg]\nv = \"1\"\n").unwrap();
    fs::write(root.join("README.md"), "# Title\n\n## Sub\n").unwrap();

    // Edge: gitignored.
    fs::write(root.join(".gitignore"), "ignored.rs\nbuild/\n").unwrap();
    fs::write(root.join("ignored.rs"), "fn ignored() {}\n").unwrap();

    // Edge: oversized.
    let big = vec![b'a'; (2 * 1024 * 1024 + 1) as usize];
    fs::write(root.join("src/huge.rs"), &big).unwrap();

    // Edge: non-UTF-8.
    fs::write(root.join("src/bad.rs"), [0xff, 0xfe, 0xfd, 0xfc]).unwrap();

    // Edge: nested subdir.
    fs::create_dir(root.join("src/nested")).unwrap();
    fs::write(root.join("src/nested/inner.rs"), "fn inner() {}\n").unwrap();

    // Edge: will be deleted mid-flow.
    fs::write(root.join("src/doomed.rs"), "fn doomed() {}\n").unwrap();

    tmp
}

fn cmd_at(root: &Path) -> Command {
    let mut c = Command::cargo_bin("repoctx").unwrap();
    c.args(["--repo", root.to_str().unwrap()]);
    c
}

fn run_json(root: &Path, subcmd: &[&str]) -> Value {
    let mut c = cmd_at(root);
    c.arg("--json").args(subcmd);
    let out = c.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn full_flow_index_symbols_status_reindex() {
    let tmp = full_fixture();
    let root = tmp.path();

    // 1. First index pass.
    let s1 = run_json(root, &["index"]);
    // 10 indexable files: main.rs, lib.go, a.ts, a.tsx, a.js, a.py, config.json,
    // config.yaml, Config.toml, README.md, src/nested/inner.rs, src/doomed.rs
    // = 12. ignored.rs gitignored; huge.rs >2 MiB; bad.rs non-UTF-8.
    assert_eq!(s1["indexed"], 12, "{s1}");
    assert_eq!(s1["unchanged"], 0);
    assert_eq!(s1["removed"], 0);
    assert!(root.join(".repoctx/index.db").exists());

    // 2. symbols across languages.
    let v = run_json(root, &["symbols", "Cat"]);
    let names: Vec<_> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["name"].as_str().unwrap(), i["kind"].as_str().unwrap()))
        .collect();
    assert!(names.iter().any(|(n, _)| *n == "Cat"), "{names:?}");

    // 3. --kind filter.
    let v = run_json(root, &["symbols", "Cat", "--kind", "class"]);
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["kind"] == "class"),
        "{v}"
    );

    // 4. --lang filter.
    let v = run_json(root, &["symbols", "hello", "--lang", "javascript"]);
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items
        .iter()
        .all(|i| i["location"]["path"].as_str().unwrap().ends_with(".js")));

    // 5. empty result is success.
    let v = run_json(root, &["symbols", "definitelynothere"]);
    assert_eq!(v["count"], 0);
    assert_eq!(v["items"].as_array().unwrap().len(), 0);

    // 6. status reports the 12 files + per-language + zero staleness.
    let st = run_json(root, &["status"]);
    assert_eq!(st["files"], 12);
    let langs: Vec<_> = st["per_language"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["language"].as_str().unwrap())
        .collect();
    for expected in [
        "go",
        "javascript",
        "json",
        "markdown",
        "python",
        "rust",
        "toml",
        "tsx",
        "typescript",
        "yaml",
    ] {
        assert!(langs.contains(&expected), "missing {expected} in {langs:?}");
    }
    assert_eq!(st["staleness"]["changed"], 0);
    assert_eq!(st["staleness"]["new"], 0);
    assert_eq!(st["staleness"]["deleted"], 0);

    // 7. Edit one file + delete another + add a new one, re-index.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    fs::remove_file(root.join("src/doomed.rs")).unwrap();
    fs::write(root.join("src/newcomer.rs"), "fn newcomer() {}\n").unwrap();

    // status (pre-reindex) sees the deltas.
    let st = run_json(root, &["status"]);
    assert_eq!(st["staleness"]["changed"], 1);
    assert_eq!(st["staleness"]["new"], 1);
    assert_eq!(st["staleness"]["deleted"], 1);

    let s2 = run_json(root, &["index"]);
    assert_eq!(s2["indexed"], 2, "{s2}"); // edited + new
    assert_eq!(s2["removed"], 1, "{s2}");
    assert!(s2["unchanged"].as_u64().unwrap() >= 10);

    // status now clean again.
    let st = run_json(root, &["status"]);
    assert_eq!(st["staleness"]["changed"], 0);
    assert_eq!(st["staleness"]["new"], 0);
    assert_eq!(st["staleness"]["deleted"], 0);

    // 8. Deleted file's symbols are gone.
    let v = run_json(root, &["symbols", "doomed"]);
    assert_eq!(v["count"], 0);
}

#[test]
fn query_commands_have_clean_stderr() {
    let tmp = full_fixture();
    let root = tmp.path();
    cmd_at(root).args(["--json", "index"]).assert().success(); // ignore stderr here — skip warnings expected

    for sub in [
        vec!["--json", "symbols", "main"],
        vec!["--json", "status"],
        vec!["--json", "status", "--fast"],
    ] {
        let out = cmd_at(root)
            .args(&sub)
            .assert()
            .success()
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.is_empty(), "query {sub:?} produced stderr: {stderr}");
    }
}

#[test]
fn index_stderr_warns_only_for_skipped_files() {
    let tmp = full_fixture();
    let root = tmp.path();
    let out = cmd_at(root)
        .args(["--json", "index"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Each warning line carries the file basename plus a known phrase.
    let mut saw_huge = false;
    let mut saw_bad = false;
    for line in stderr.lines() {
        let allowed = line.contains("skipping file > 2 MiB") || line.contains("non-UTF-8");
        if !allowed {
            panic!("unexpected stderr line: {line}\nfull:\n{stderr}");
        }
        if line.contains("huge.rs") {
            saw_huge = true;
        }
        if line.contains("bad.rs") {
            saw_bad = true;
        }
    }
    assert!(saw_huge && saw_bad, "missing expected warnings: {stderr}");
}

#[test]
fn force_reparses_every_file() {
    let tmp = full_fixture();
    let root = tmp.path();
    run_json(root, &["index"]);
    let v = run_json(root, &["index", "--force"]);
    assert_eq!(v["unchanged"], 0);
    assert_eq!(v["indexed"], 12);
}
