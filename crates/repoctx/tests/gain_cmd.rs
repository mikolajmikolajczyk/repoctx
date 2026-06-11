//! E2E for `repoctx gain` + `gain top`.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use rusqlite::{params, Connection};
use serde_json::Value;
use tempfile::TempDir;

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64
}

fn fixture() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    fs::write(tmp.path().join("a.rs"), "fn a() {}\n").unwrap();
    tmp
}

fn index(root: &Path) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", root.to_str().unwrap(), "--json", "index"])
        .assert()
        .success();
}

fn seed(root: &Path, rows: &[(i64, &str, u32, i64, i64, i64, &str)]) {
    let conn = Connection::open(root.join(".repoctx/index.db")).unwrap();
    for (ts, cmd, cf, cb, est, ret, fmt) in rows {
        conn.execute(
            "INSERT INTO usage(ts_unix_ns, command, candidate_files, candidate_bytes,
                               estimated_baseline_tokens, returned_tokens, output_format, query)
             VALUES(?1,?2,?3,?4,?5,?6,?7,NULL)",
            params![ts, cmd, cf, cb, est, ret, fmt],
        )
        .unwrap();
    }
}

fn run(root: &Path, args: &[&str]) -> Value {
    let mut c = Command::cargo_bin("repoctx").unwrap();
    c.args(["--repo", root.to_str().unwrap(), "--json"])
        .args(args);
    let out = c.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn no_auto_index_flag_preserves_error() {
    let tmp = fixture();
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args([
            "--repo",
            tmp.path().to_str().unwrap(),
            "--no-auto-index",
            "gain",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no index found"), "{stderr}");
}

#[test]
fn missing_index_auto_indexes_then_runs_gain() {
    let tmp = fixture();
    let v = run(tmp.path(), &["gain", "--all"]);
    assert_eq!(v["commands"], 0);
    assert!(tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn empty_usage_returns_zero_summary() {
    let tmp = fixture();
    index(tmp.path());
    let v = run(tmp.path(), &["gain"]);
    assert_eq!(v["commands"], 0);
    assert_eq!(v["returned_tokens"], 0);
    assert_eq!(v["estimated_baseline_tokens"], 0);
    assert_eq!(v["estimated_savings"], 0);
}

#[test]
fn summary_aggregates_seeded_rows() {
    let tmp = fixture();
    index(tmp.path());
    let now = now_ns();
    seed(
        tmp.path(),
        &[
            (now - 1_000, "symbols", 1, 4_000, 1_000, 50, "json"),
            (now - 500, "symbols", 1, 6_000, 1_500, 100, "toon"),
            (now - 200, "outline", 1, 2_000, 500, 20, "human"),
        ],
    );
    let v = run(tmp.path(), &["gain", "--all"]);
    assert_eq!(v["commands"], 3);
    assert_eq!(v["estimated_baseline_tokens"], 3_000);
    assert_eq!(v["returned_tokens"], 170);
    assert_eq!(v["estimated_savings"], 2_830);
    let red = v["reduction"].as_f64().unwrap();
    assert!((red - 94.3).abs() < 0.2, "reduction={red}");
}

#[test]
fn since_window_filters() {
    let tmp = fixture();
    index(tmp.path());
    let now = now_ns();
    let one_day = 86_400i64 * 1_000_000_000;
    seed(
        tmp.path(),
        &[
            (now - 10 * one_day, "symbols", 1, 9_999, 2_500, 10, "json"),
            (now - 1_000, "symbols", 1, 100, 25, 1, "json"),
        ],
    );
    let v = run(tmp.path(), &["gain", "--since", "1d"]);
    assert_eq!(v["commands"], 1);
    assert_eq!(v["estimated_baseline_tokens"], 25);
}

#[test]
fn history_lists_recent_rows() {
    let tmp = fixture();
    index(tmp.path());
    let now = now_ns();
    seed(
        tmp.path(),
        &[
            (now - 300, "symbols", 1, 100, 25, 5, "json"),
            (now - 200, "symbols", 1, 200, 50, 10, "json"),
            (now - 100, "outline", 1, 300, 75, 15, "human"),
        ],
    );
    let v = run(tmp.path(), &["gain", "--history", "2"]);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["command"], "outline");
    assert_eq!(items[1]["command"], "symbols");
}

#[test]
fn top_orders_by_saved_by_default() {
    let tmp = fixture();
    index(tmp.path());
    let now = now_ns();
    seed(
        tmp.path(),
        &[
            (now, "symbols", 1, 1_000, 250, 50, "json"),
            (now, "outline", 1, 100, 25, 1, "json"),
            (now, "definition", 1, 5_000, 1_250, 200, "json"),
        ],
    );
    let v = run(tmp.path(), &["gain", "top", "--all"]);
    let items = v["items"].as_array().unwrap();
    let order: Vec<_> = items
        .iter()
        .map(|x| x["command"].as_str().unwrap())
        .collect();
    assert_eq!(order, vec!["definition", "symbols", "outline"]);
    assert_eq!(v["by"], "saved");
}

#[test]
fn top_by_ratio_reorders() {
    let tmp = fixture();
    index(tmp.path());
    let now = now_ns();
    // Crafted so 'outline' has highest ratio (96%), 'symbols' has highest absolute savings.
    seed(
        tmp.path(),
        &[
            (now, "symbols", 1, 10_000, 2_500, 1_000, "json"), // reduction 60.0
            (now, "outline", 1, 100, 25, 1, "json"),           // reduction 96.0
        ],
    );
    let v = run(tmp.path(), &["gain", "top", "--by", "ratio", "--all"]);
    let items = v["items"].as_array().unwrap();
    let order: Vec<_> = items
        .iter()
        .map(|x| x["command"].as_str().unwrap())
        .collect();
    assert_eq!(order, vec!["outline", "symbols"]);
    assert_eq!(v["by"], "ratio");
}

#[test]
fn gain_is_not_itself_recorded() {
    let tmp = fixture();
    index(tmp.path());
    Command::cargo_bin("repoctx")
        .unwrap()
        .args(["--repo", tmp.path().to_str().unwrap(), "--json", "gain"])
        .assert()
        .success();
    let conn = Connection::open(tmp.path().join(".repoctx/index.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM usage", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
