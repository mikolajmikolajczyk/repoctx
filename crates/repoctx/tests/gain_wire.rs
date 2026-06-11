//! E2E coverage for gain recording on `repoctx symbols`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(
        root.join("a.rs"),
        "fn alpha() {}\nfn beta() {}\nstruct Gamma;\n",
    )
    .unwrap();
    fs::write(root.join("b.rs"), "fn alpha_x() {}\n").unwrap();
    tmp
}

fn index(root: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
}

fn usage_count(root: &Path) -> i64 {
    let conn = Connection::open(root.join(".repoctx/index.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM usage", [], |r| r.get(0))
        .unwrap()
}

fn last_usage_row(root: &Path) -> (String, u32, i64, i64, Option<String>) {
    let conn = Connection::open(root.join(".repoctx/index.db")).unwrap();
    conn.query_row(
        "SELECT command, candidate_files, candidate_bytes, returned_tokens, query
         FROM usage ORDER BY id DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .unwrap()
}

fn symbols(root: &Path, extra: &[&str]) {
    let mut c = Command::cargo_bin("repoctx").unwrap();
    c.args(["--repo", root.to_str().unwrap(), "--json", "symbols"]);
    c.args(extra);
    c.assert().success();
}

#[test]
fn symbols_inserts_one_usage_row_per_invocation() {
    let tmp = fixture();
    index(tmp.path());
    assert_eq!(usage_count(tmp.path()), 0);
    symbols(tmp.path(), &["alpha"]);
    assert_eq!(usage_count(tmp.path()), 1);
    symbols(tmp.path(), &["alpha"]);
    assert_eq!(usage_count(tmp.path()), 2);
    let (cmd, cf, cb, ret, q) = last_usage_row(tmp.path());
    assert_eq!(cmd, "symbols");
    assert_eq!(cf, 2);
    assert!(cb > 0);
    assert!(ret > 0);
    assert_eq!(q, None);
}

#[test]
fn no_record_flag_skips_insertion() {
    let tmp = fixture();
    index(tmp.path());
    symbols(tmp.path(), &["alpha", "--no-record"]);
    assert_eq!(usage_count(tmp.path()), 0);
}

#[test]
fn env_var_skips_insertion() {
    let tmp = fixture();
    index(tmp.path());
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--json",
            "symbols",
            "alpha",
        ])
        .assert()
        .success();
    assert_eq!(usage_count(tmp.path()), 0);
}

#[test]
fn record_query_persists_text_default_leaves_null() {
    let tmp = fixture();
    index(tmp.path());
    symbols(tmp.path(), &["UserService"]);
    assert_eq!(last_usage_row(tmp.path()).4, None);

    symbols(tmp.path(), &["UserService", "--record-query"]);
    assert_eq!(
        last_usage_row(tmp.path()).4,
        Some("UserService".to_string())
    );
}

#[test]
fn recording_failure_does_not_break_query_output() {
    // Corrupt the usage table by dropping it AFTER indexing. The next
    // symbols call should still print results to stdout (recording is
    // fire-and-forget per contract).
    let tmp = fixture();
    index(tmp.path());
    {
        let conn = Connection::open(tmp.path().join(".repoctx/index.db")).unwrap();
        conn.execute("DROP TABLE usage", []).unwrap();
    }
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--json",
            "symbols",
            "alpha",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(body.contains("alpha"), "stdout missing 'alpha': {body}");
}
