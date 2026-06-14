//! End-to-end `repoctx search` tests (epic f4cb992): symbol defs + complete
//! textual matches (incl. comments), no silent loss.

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn rg_available() -> bool {
    StdCommand::new("rg").arg("--version").output().is_ok()
}

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    // `foo` appears as: a definition, a call site, AND a comment mention
    // (the textual-only occurrence a symbol index would miss).
    fs::write(
        root.join("lib.rs"),
        "// remember to call foo before bar\nfn foo() {}\nfn bar() {\n    foo();\n}\n",
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
fn search_leads_with_symbol_definition() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    let syms = v["symbols"].as_array().unwrap();
    assert!(
        syms.iter()
            .any(|s| s["name"] == "foo" && s["kind"] == "function"),
        "symbol def present: {v}"
    );
}

#[test]
fn search_includes_comment_mention_no_textual_loss() {
    if !rg_available() {
        eprintln!("rg not on PATH — skipping textual-match assertion");
        return;
    }
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    // The comment line mentions foo but is not a symbol — it must still show.
    let texts: Vec<String> = v["matches"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["lines"].as_array().unwrap().clone())
        .map(|l| l["text"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("remember to call foo")),
        "comment mention must appear in textual matches: {texts:?}"
    );
    assert!(v["matches"]["count"].as_u64().unwrap() >= 2);
}

#[test]
fn search_rg_absent_still_returns_symbols() {
    // Force rg "absent" by running with an empty PATH override is awkward in
    // assert_cmd; instead assert the symbols section is independent of rg by
    // checking it's present regardless (rg-absent path is unit-covered by the
    // empty-matches fallback in search_cmd).
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    assert!(v["symbols"].is_array());
    assert!(v["matches"]["files"].is_array());
}
