//! Rewrite-decision corpus, asserted through the shipping CLI entry
//! point `repoctx hook claude` (issue 573eccc).
//!
//! The same corpus file is run through the pure `try_semantic_rewrite`
//! in a unit test (src/hook_rewrite.rs). This test proves the assembled
//! binary makes the identical decision: a JSON PreToolUse payload in,
//! exit 0 + rewritten `updatedInput.command` on rewrite, exit 1 on
//! passthrough. The two entry points must agree.

use std::path::PathBuf;

use assert_cmd::Command;
use serde::Deserialize;
use serde_json::{json, Value};
use tempfile::TempDir;

#[derive(Deserialize)]
struct Corpus {
    case: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    cmd: String,
    expect: String,
    #[serde(default)]
    to: Option<String>,
}

fn load() -> Vec<Case> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rewrite_corpus.toml");
    let text = std::fs::read_to_string(&path).expect("read corpus");
    toml::from_str::<Corpus>(&text).expect("parse corpus").case
}

/// Pipe one command through `repoctx hook claude` and return
/// (exit_code, stdout). `repo` is an empty tempdir so no chain commands
/// and no recorded config interfere.
fn drive(repo: &TempDir, cmd: &str) -> (i32, Vec<u8>) {
    let payload = json!({ "tool_input": { "command": cmd } }).to_string();
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .arg("--repo")
        .arg(repo.path())
        // --rtk-chain=0 isolates the rewrite decision from rtk chaining
        // (this test asserts the decision, not chain delegation).
        .args(["hook", "claude", "--rtk-chain", "0"])
        .write_stdin(payload)
        .output()
        .expect("run hook claude");
    (out.status.code().unwrap_or(-1), out.stdout)
}

fn rewritten_command(stdout: &[u8]) -> String {
    let v: Value = serde_json::from_slice(stdout).expect("hook output is JSON");
    v["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .expect("updatedInput.command present")
        .to_string()
}

#[test]
fn corpus_cli_decisions_match() {
    let repo = tempfile::tempdir().unwrap();
    let cases = load();
    assert!(cases.len() >= 100, "corpus has {} rows", cases.len());

    for c in &cases {
        let (code, stdout) = drive(&repo, &c.cmd);
        match c.expect.as_str() {
            "rewrite" => {
                assert_eq!(code, 0, "expected REWRITE (exit 0) for `{}`", c.cmd);
                let got = rewritten_command(&stdout);
                let want = c.to.as_deref().expect("rewrite row needs `to`");
                assert_eq!(got, want, "rewritten command mismatch for `{}`", c.cmd);
            }
            "passthrough" => {
                assert_eq!(
                    code,
                    1,
                    "expected PASSTHROUGH (exit 1) for `{}`, stdout={}",
                    c.cmd,
                    String::from_utf8_lossy(&stdout)
                );
                assert!(
                    stdout.is_empty(),
                    "passthrough must emit no payload for `{}`",
                    c.cmd
                );
            }
            other => panic!("bad `expect` `{other}` for `{}`", c.cmd),
        }
    }
}
