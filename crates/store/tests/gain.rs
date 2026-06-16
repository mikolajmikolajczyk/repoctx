use repoctx_store::{
    CommandBreakdown, FileRecord, GainTotals, Store, SymbolRecord, UsageRecord, SUPPORTED_VERSION,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn fr(path: &str, size: i64) -> FileRecord {
    FileRecord {
        path: path.into(),
        mtime_ns: 0,
        size,
        language: "rust".into(),
    }
}

fn sr(file: &str, name: &str) -> SymbolRecord {
    SymbolRecord {
        file_path: file.into(),
        name: name.into(),
        kind: "function".into(),
        start_line: 0,
        start_column: 0,
        end_line: 0,
        end_column: 5,
        visibility: "unknown".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn rec(
    ts: i64,
    cmd: &str,
    cf: u32,
    cb: i64,
    est: i64,
    ret: i64,
    fmt: &str,
    q: Option<&str>,
) -> UsageRecord {
    UsageRecord {
        ts_unix_ns: ts,
        command: cmd.into(),
        candidate_files: cf,
        candidate_bytes: cb,
        estimated_baseline_tokens: est,
        returned_tokens: ret,
        output_format: fmt.into(),
        query: q.map(str::to_string),
    }
}

#[test]
fn schema_on_fresh_open() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(s.schema_version().unwrap(), SUPPORTED_VERSION);
    assert_eq!(SUPPORTED_VERSION, 8);
}

#[test]
fn v1_db_migrates_to_latest_with_rows_intact() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("legacy.db");

    // Hand-build a real v1 DB matching the v1 migration verbatim.
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE files (
                path     TEXT PRIMARY KEY,
                mtime_ns INTEGER NOT NULL,
                size     INTEGER NOT NULL,
                language TEXT NOT NULL
            );
            CREATE TABLE symbols (
                id           INTEGER PRIMARY KEY,
                file_path    TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                name         TEXT NOT NULL,
                kind         TEXT NOT NULL,
                start_line   INTEGER NOT NULL,
                start_column INTEGER NOT NULL,
                end_line     INTEGER NOT NULL,
                end_column   INTEGER NOT NULL
            );
            CREATE INDEX symbols_name_idx      ON symbols(name);
            CREATE INDEX symbols_file_path_idx ON symbols(file_path);
            CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
            INSERT INTO meta(key, value) VALUES('schema_version', '1');
            INSERT INTO files(path, mtime_ns, size, language)
              VALUES('a.rs', 1, 100, 'rust');
            INSERT INTO symbols(file_path, name, kind, start_line, start_column, end_line, end_column)
              VALUES('a.rs', 'foo', 'function', 0, 0, 0, 3);
            "#,
        )
        .unwrap();
    }

    // Open through Store -> should migrate to latest in place.
    let s = Store::open_at(&path).unwrap();
    assert_eq!(s.schema_version().unwrap(), SUPPORTED_VERSION);
    // Original rows preserved.
    let by_file = s.symbols_by_file("a.rs").unwrap();
    assert_eq!(by_file.len(), 1);
    assert_eq!(by_file[0].name, "foo");
    let mtimes = s.file_mtimes().unwrap();
    assert_eq!(mtimes["a.rs"], (1, 100));
}

#[test]
fn record_and_totals_round_trip() {
    let mut s = Store::open_in_memory().unwrap();
    s.record_usage(&rec(100, "symbols", 2, 10_000, 2_500, 200, "json", None))
        .unwrap();
    s.record_usage(&rec(200, "symbols", 1, 5_000, 1_250, 50, "json", None))
        .unwrap();
    s.record_usage(&rec(300, "outline", 1, 4_000, 1_000, 80, "human", None))
        .unwrap();

    let t = s.gain_totals(None).unwrap();
    assert_eq!(
        t,
        GainTotals {
            invocations: 3,
            candidate_bytes: 19_000,
            estimated_baseline_tokens: 4_750,
            returned_tokens: 330,
        }
    );

    // Windowed: drop the earliest.
    let t = s.gain_totals(Some(150)).unwrap();
    assert_eq!(t.invocations, 2);
    assert_eq!(t.candidate_bytes, 9_000);
}

#[test]
fn per_command_breakdown_orders_alphabetically() {
    let mut s = Store::open_in_memory().unwrap();
    s.record_usage(&rec(1, "symbols", 1, 100, 25, 10, "json", None))
        .unwrap();
    s.record_usage(&rec(2, "outline", 1, 200, 50, 20, "json", None))
        .unwrap();
    s.record_usage(&rec(3, "symbols", 1, 300, 75, 30, "json", None))
        .unwrap();

    let rows = s.gain_per_command(None).unwrap();
    assert_eq!(
        rows,
        vec![
            CommandBreakdown {
                command: "outline".into(),
                invocations: 1,
                candidate_bytes: 200,
                estimated_baseline_tokens: 50,
                returned_tokens: 20,
            },
            CommandBreakdown {
                command: "symbols".into(),
                invocations: 2,
                candidate_bytes: 400,
                estimated_baseline_tokens: 100,
                returned_tokens: 40,
            },
        ]
    );
}

#[test]
fn recent_orders_newest_first_and_respects_limit() {
    let mut s = Store::open_in_memory().unwrap();
    for i in 0..5 {
        s.record_usage(&rec(i, "symbols", 1, 100, 25, 10, "json", None))
            .unwrap();
    }
    let r = s.gain_recent(3).unwrap();
    assert_eq!(r.len(), 3);
    assert_eq!(r[0].ts_unix_ns, 4);
    assert_eq!(r[1].ts_unix_ns, 3);
    assert_eq!(r[2].ts_unix_ns, 2);
}

#[test]
fn baseline_for_files_sums_known_paths_and_skips_unknown() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("a.rs", 100), &[sr("a.rs", "x")]).unwrap();
    s.upsert_file(&fr("b.rs", 250), &[sr("b.rs", "y")]).unwrap();

    let (cf, cb) = s
        .gain_baseline_for_files(&["a.rs".into(), "b.rs".into(), "ghost.rs".into()])
        .unwrap();
    assert_eq!(cf, 2);
    assert_eq!(cb, 350);

    // Empty input is zero, not an error.
    assert_eq!(s.gain_baseline_for_files(&[]).unwrap(), (0, 0));
    // Only unknown -> zero.
    assert_eq!(
        s.gain_baseline_for_files(&["ghost.rs".into()]).unwrap(),
        (0, 0)
    );
}

#[test]
fn query_text_persists_only_when_provided() {
    let mut s = Store::open_in_memory().unwrap();
    s.record_usage(&rec(1, "symbols", 1, 100, 25, 10, "json", None))
        .unwrap();
    s.record_usage(&rec(
        2,
        "symbols",
        1,
        100,
        25,
        10,
        "json",
        Some("UserService"),
    ))
    .unwrap();
    let r = s.gain_recent(2).unwrap();
    assert_eq!(r[0].query.as_deref(), Some("UserService"));
    assert_eq!(r[1].query, None);
}

#[test]
fn privacy_no_filenames_or_symbol_names_in_usage_table() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("p.db");
    {
        let mut s = Store::open_at(&path).unwrap();
        s.upsert_file(
            &fr("very_private_filename.rs", 100),
            &[sr("very_private_filename.rs", "secret_symbol")],
        )
        .unwrap();
        s.record_usage(&rec(1, "symbols", 1, 100, 25, 10, "json", None))
            .unwrap();
    }
    // Dump usage table via raw rusqlite — make sure NO filename / symbol name leaked.
    let conn = Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT ts_unix_ns, command, candidate_files, candidate_bytes, estimated_baseline_tokens, returned_tokens, output_format, query FROM usage")
        .unwrap();
    let dump: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let blob = dump.join("\n");
    assert!(
        !blob.contains("very_private_filename"),
        "leaked filename: {blob}"
    );
    assert!(!blob.contains("secret_symbol"), "leaked symbol: {blob}");
}
