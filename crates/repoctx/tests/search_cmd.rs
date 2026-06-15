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
        "// remember to call foo before bar\nfn foo() {}\nfn bar() {\n    foo();\n    ext_thing();\n}\nfn foobar() {\n    foo();\n}\n",
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

fn items(v: &Value) -> Vec<Value> {
    v["results"].as_array().unwrap().clone()
}

#[test]
fn structural_item_is_the_symbol_definition() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    let structural: Vec<Value> = items(&v)
        .into_iter()
        .filter(|r| r["source"] == "structural")
        .collect();
    assert!(
        structural
            .iter()
            .any(|s| s["name"] == "foo" && s["kind"] == "function"),
        "structural def present: {v}"
    );
}

#[test]
fn comment_mention_is_tagged_textual_no_loss() {
    if !rg_available() {
        eprintln!("rg not on PATH — skipping textual assertion");
        return;
    }
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    // The comment line mentions foo but is not a symbol — present + tagged textual.
    assert!(
        items(&v).iter().any(|r| {
            r["source"] == "textual"
                && r["text"]
                    .as_str()
                    .unwrap_or("")
                    .contains("remember to call foo")
        }),
        "comment mention must appear, tagged textual: {v}"
    );
}

#[test]
fn call_site_is_tagged_reference() {
    if !rg_available() {
        eprintln!("rg not on PATH — skipping reference assertion");
        return;
    }
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    // `foo()` inside bar is a call site → reference, not plain textual.
    assert!(
        items(&v).iter().any(|r| {
            r["source"] == "reference" && r["text"].as_str().unwrap_or("").contains("foo()")
        }),
        "call site must be tagged reference: {v}"
    );
}

#[test]
fn structural_item_surfaces_callers() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    // foo is called by bar — under callers.internal (caller is an indexed sym).
    let foo = items(&v)
        .into_iter()
        .find(|r| r["source"] == "structural" && r["name"] == "foo")
        .expect("structural foo");
    let internal = foo["callers"]["internal"]
        .as_array()
        .expect("callers.internal");
    assert!(
        internal.iter().any(|c| c["name"] == "bar"),
        "foo's internal callers must include bar: {foo}"
    );
}

#[test]
fn internal_callee_shown_external_collapsed_by_default() {
    let tmp = fixture();
    index(tmp.path());
    // bar calls foo (internal) AND ext_thing (external — not in the index).
    let v = json(tmp.path(), &["search", "bar"]);
    let bar = items(&v)
        .into_iter()
        .find(|r| r["source"] == "structural" && r["name"] == "bar")
        .expect("structural bar");
    let callees = &bar["callees"];
    assert!(
        callees["internal"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "foo"),
        "internal callee foo expanded: {bar}"
    );
    // external collapses to a count, names not expanded by default.
    assert!(callees["external_count"].as_u64().unwrap() >= 1, "{bar}");
    assert!(
        callees.get("external").is_none() || callees["external"].as_array().unwrap().is_empty(),
        "external names collapsed by default: {bar}"
    );
}

#[test]
fn all_callees_expands_external_names() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "bar", "--all-callees"]);
    let bar = items(&v)
        .into_iter()
        .find(|r| r["source"] == "structural" && r["name"] == "bar")
        .expect("structural bar");
    let external = bar["callees"]["external"]
        .as_array()
        .expect("external names");
    assert!(
        external.iter().any(|n| n == "ext_thing"),
        "--all-callees expands external names: {bar}"
    );
}

#[test]
fn non_exact_structural_also_carries_call_edges() {
    let tmp = fixture();
    index(tmp.path());
    // Query `foo`; `foobar` is a substring (non-exact) structural match. It
    // must carry its OWN call edges — not only the exact-name `foo` does.
    let v = json(tmp.path(), &["search", "foo"]);
    let foobar = items(&v)
        .into_iter()
        .find(|r| r["source"] == "structural" && r["name"] == "foobar")
        .expect("structural foobar");
    assert!(
        foobar["callees"]["internal"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "foo"),
        "foobar's own callees must include foo: {foobar}"
    );
}

#[test]
fn structural_present_without_rg() {
    let tmp = fixture();
    index(tmp.path());
    let v = json(tmp.path(), &["search", "foo"]);
    assert!(v["results"].is_array());
    assert!(items(&v).iter().any(|r| r["source"] == "structural"));
}
