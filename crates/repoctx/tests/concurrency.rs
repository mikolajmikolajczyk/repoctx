//! Concurrency + corruption + schema-mismatch coverage for the CLI.

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use std::thread;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    for i in 0..20 {
        fs::write(
            root.join(format!("f{i}.rs")),
            format!("fn f{i}() {{}}\nstruct S{i};\n"),
        )
        .unwrap();
    }
    tmp
}

fn bin_path() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("repoctx")
}

#[test]
fn parallel_index_invocations_complete_cleanly() {
    // Contract: both runs succeed (serializing on busy_timeout), OR if the
    // 5s timeout is exceeded under load, the loser exits 1 with the typed
    // 'index is locked' message — never with a raw rusqlite panic.
    let tmp = fixture();
    let root = tmp.path().to_path_buf();
    for _round in 0..3 {
        let h1 = {
            let r = root.clone();
            thread::spawn(move || {
                StdCommand::new(bin_path())
                    .args(["--repo", r.to_str().unwrap(), "--json", "index", "--force"])
                    .output()
                    .unwrap()
            })
        };
        let h2 = {
            let r = root.clone();
            thread::spawn(move || {
                StdCommand::new(bin_path())
                    .args(["--repo", r.to_str().unwrap(), "--json", "index", "--force"])
                    .output()
                    .unwrap()
            })
        };
        assert_ok_or_locked(&h1.join().unwrap());
        assert_ok_or_locked(&h2.join().unwrap());
    }
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "symbols", "f0"])
        .assert()
        .success();
}

fn assert_ok_or_locked(out: &std::process::Output) {
    if out.status.success() {
        return;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index is locked"),
        "unexpected failure (exit {:?}): {stderr}",
        out.status.code()
    );
}

#[test]
fn reader_during_writer_uses_wal_snapshot() {
    let tmp = fixture();
    let root = tmp.path().to_path_buf();
    // Seed an index.
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
    // Spawn a long-ish writer (force re-parse all) and a reader concurrently.
    let r = root.clone();
    let writer = thread::spawn(move || {
        StdCommand::new(bin_path())
            .args(["--repo", r.to_str().unwrap(), "--json", "index", "--force"])
            .status()
            .unwrap()
    });
    let r = root.clone();
    let reader = thread::spawn(move || {
        StdCommand::new(bin_path())
            .args(["--repo", r.to_str().unwrap(), "--json", "symbols", "f0"])
            .status()
            .unwrap()
    });
    assert!(writer.join().unwrap().success());
    assert!(reader.join().unwrap().success());
}

fn write_meta_version(path: &Path, version: u32) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES('schema_version', ?1)",
        rusqlite::params![version.to_string()],
    )
    .unwrap();
}

#[test]
fn newer_schema_version_rejects_with_clean_message() {
    let tmp = fixture();
    let root = tmp.path();
    // Build a real v1 DB.
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
    // Bump schema_version directly.
    write_meta_version(&root.join(".repoctx/index.db"), 99);

    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "symbols", "f0"])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index was created by a newer repoctx"),
        "stderr was: {stderr}"
    );
}

#[test]
fn corrupted_db_rejects_with_clean_message_no_autodelete() {
    let tmp = fixture();
    let root = tmp.path();
    fs::create_dir(root.join(".repoctx")).unwrap();
    fs::write(root.join(".repoctx/index.db"), b"this is not a database").unwrap();

    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "symbols", "f0"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index is corrupted"),
        "stderr was: {stderr}"
    );
    // File NOT deleted — user keeps the broken state for inspection.
    assert!(root.join(".repoctx/index.db").exists());
}
