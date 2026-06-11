use std::path::PathBuf;

use repoctx_store::{
    from_db_path, to_db_path, FileRecord, Store, SymbolFilter, SymbolRecord, SUPPORTED_VERSION,
};
use tempfile::tempdir;

fn fr(path: &str, mtime: i64, size: i64, lang: &str) -> FileRecord {
    FileRecord {
        path: path.into(),
        mtime_ns: mtime,
        size,
        language: lang.into(),
    }
}

fn sr(file: &str, name: &str, kind: &str, line: u32) -> SymbolRecord {
    SymbolRecord {
        file_path: file.into(),
        name: name.into(),
        kind: kind.into(),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 10,
    }
}

#[test]
fn schema_version_after_open() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(s.schema_version().unwrap(), SUPPORTED_VERSION);
}

#[test]
fn open_creates_dot_repoctx() {
    let tmp = tempdir().unwrap();
    let _s = Store::open(tmp.path()).unwrap();
    assert!(tmp.path().join(".repoctx/index.db").exists());
}

#[test]
fn migration_is_idempotent() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("idem.db");
    let s1 = Store::open_at(&db).unwrap();
    let v1 = s1.schema_version().unwrap();
    drop(s1);
    let s2 = Store::open_at(&db).unwrap();
    assert_eq!(s2.schema_version().unwrap(), v1);
}

#[test]
fn upsert_replaces_old_symbols() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 10, "rust"),
        &[
            sr("a.rs", "foo", "function", 1),
            sr("a.rs", "bar", "function", 2),
        ],
    )
    .unwrap();
    s.upsert_file(
        &fr("a.rs", 2, 11, "rust"),
        &[sr("a.rs", "baz", "function", 3)],
    )
    .unwrap();
    let by_file = s.symbols_by_file("a.rs").unwrap();
    assert_eq!(by_file.len(), 1);
    assert_eq!(by_file[0].name, "baz");
    let mtimes = s.file_mtimes().unwrap();
    assert_eq!(mtimes["a.rs"], (2, 11));
}

#[test]
fn cascade_delete_on_prune() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("a.rs", 1, 1, "rust"), &[sr("a.rs", "x", "function", 1)])
        .unwrap();
    s.upsert_file(&fr("b.rs", 1, 1, "rust"), &[sr("b.rs", "y", "function", 1)])
        .unwrap();
    assert_eq!(s.prune(&["a.rs".into()]).unwrap(), 1);
    assert!(s.symbols_by_file("a.rs").unwrap().is_empty());
    assert_eq!(s.symbols_by_file("b.rs").unwrap().len(), 1);
}

#[test]
fn substring_ordering_is_deterministic() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("z.rs", 1, 1, "rust"),
        &[
            sr("z.rs", "foo_a", "function", 5),
            sr("z.rs", "foo_a", "function", 2),
        ],
    )
    .unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 1, "rust"),
        &[
            sr("a.rs", "foo_a", "function", 10),
            sr("a.rs", "foo_b", "function", 1),
        ],
    )
    .unwrap();
    let hits = s
        .symbols_substring("foo", &SymbolFilter::default())
        .unwrap();
    let order: Vec<_> = hits
        .iter()
        .map(|(s, _)| (s.name.as_str(), s.file_path.as_str(), s.start_line))
        .collect();
    assert_eq!(
        order,
        vec![
            ("foo_a", "a.rs", 10),
            ("foo_a", "z.rs", 2),
            ("foo_a", "z.rs", 5),
            ("foo_b", "a.rs", 1),
        ]
    );
}

#[test]
fn like_metachars_are_escaped() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 1, "rust"),
        &[
            sr("a.rs", "snake_case", "function", 1),
            sr("a.rs", "snakeXcase", "function", 2),
            sr("a.rs", "p100%done", "function", 3),
            sr("a.rs", "p100bdone", "function", 4),
        ],
    )
    .unwrap();
    let hits = s
        .symbols_substring("snake_case", &SymbolFilter::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.name, "snake_case");

    let hits = s
        .symbols_substring("100%", &SymbolFilter::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.name, "p100%done");
}

#[test]
fn substring_is_case_insensitive() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 1, "rust"),
        &[sr("a.rs", "MyThing", "function", 1)],
    )
    .unwrap();
    let hits = s
        .symbols_substring("mything", &SymbolFilter::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn filters_by_kind_and_language_and_limit() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 1, "rust"),
        &[
            sr("a.rs", "foo", "function", 1),
            sr("a.rs", "foo", "class", 2),
        ],
    )
    .unwrap();
    s.upsert_file(&fr("a.go", 1, 1, "go"), &[sr("a.go", "foo", "function", 1)])
        .unwrap();

    let filter = SymbolFilter {
        kind: Some("function"),
        ..Default::default()
    };
    let hits = s.symbols_substring("foo", &filter).unwrap();
    assert_eq!(hits.len(), 2);

    let filter = SymbolFilter {
        language: Some("go"),
        ..Default::default()
    };
    let hits = s.symbols_substring("foo", &filter).unwrap();
    assert_eq!(hits.len(), 1);

    let filter = SymbolFilter {
        limit: Some(1),
        ..Default::default()
    };
    let hits = s.symbols_substring("foo", &filter).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn counts_aggregates_per_language() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("a.rs", 1, 1, "rust"), &[sr("a.rs", "x", "function", 1)])
        .unwrap();
    s.upsert_file(&fr("b.rs", 1, 1, "rust"), &[]).unwrap();
    s.upsert_file(
        &fr("a.go", 1, 1, "go"),
        &[
            sr("a.go", "x", "function", 1),
            sr("a.go", "y", "function", 2),
        ],
    )
    .unwrap();
    let c = s.counts().unwrap();
    assert_eq!(c.files, 3);
    assert_eq!(c.symbols, 3);
    assert_eq!(c.per_language, vec![("go".into(), 1), ("rust".into(), 2)]);
}

#[test]
fn path_helpers_round_trip() {
    let p = PathBuf::from("src").join("a").join("b.rs");
    let db = to_db_path(&p);
    assert_eq!(db, "src/a/b.rs");
    assert_eq!(from_db_path(&db), p);
}
