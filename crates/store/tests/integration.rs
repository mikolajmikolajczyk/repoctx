use std::path::PathBuf;

use repoctx_store::{
    from_db_path, to_db_path, CallRecord, FileRecord, ImportRecord, Store, SymbolFilter,
    SymbolRecord, SUPPORTED_VERSION,
};
use tempfile::tempdir;

fn cr(file: &str, caller: &str, caller_line: u32, callee: &str, site_line: u32) -> CallRecord {
    CallRecord {
        file_path: file.into(),
        caller_name: caller.into(),
        caller_start_line: caller_line,
        callee_name: callee.into(),
        site_line,
        site_column: 4,
        resolution: "syntactic".into(),
    }
}

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
fn call_edges_resolve_ambiguous_and_unresolved() {
    let mut s = Store::open_in_memory().unwrap();
    // a.rs: main (line 1) calls helper + an external fn.
    s.upsert_file(
        &fr("a.rs", 1, 10, "rust"),
        &[
            sr("a.rs", "main", "function", 1),
            sr("a.rs", "helper", "function", 5),
        ],
    )
    .unwrap();
    s.upsert_calls(
        "a.rs",
        &[
            cr("a.rs", "main", 1, "helper", 2),
            cr("a.rs", "main", 1, "external_fn", 3),
        ],
    )
    .unwrap();
    // b.rs: a SECOND `helper` def — makes the callee name ambiguous.
    s.upsert_file(
        &fr("b.rs", 1, 10, "rust"),
        &[sr("b.rs", "helper", "function", 1)],
    )
    .unwrap();

    // callers_of(helper): one call site, but `helper` resolves to two defs
    // -> two rows, both with caller `main` (ambiguity surfaced as candidates).
    let callers = s.callers_of("helper").unwrap();
    assert_eq!(callers.len(), 2, "one site x two helper candidates");
    assert!(callers.iter().all(|e| e.caller.name == "main"));
    assert!(callers
        .iter()
        .all(|e| e.callee.as_ref().unwrap().name == "helper"));

    // callees_of(main): helper (2 candidates) + external_fn (unresolved).
    let callees = s.callees_of("main").unwrap();
    assert_eq!(callees.len(), 3);
    let unresolved: Vec<_> = callees.iter().filter(|e| e.callee.is_none()).collect();
    assert_eq!(unresolved.len(), 1, "external_fn is unresolved");
    assert_eq!(unresolved[0].callee_name, "external_fn");
    let helper_rows = callees.iter().filter(|e| e.callee_name == "helper").count();
    assert_eq!(helper_rows, 2, "helper resolves to two candidate defs");
}

fn ir(file: &str, module: &str, line: u32) -> ImportRecord {
    ImportRecord {
        file_path: file.into(),
        module: module.into(),
        site_line: line,
        site_column: 0,
        resolution: "syntactic".into(),
    }
}

#[test]
fn import_edges_deps_and_rdeps() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("ui.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports(
        "ui.ts",
        &[
            ir("ui.ts", "@adapters/storage-idb", 0),
            ir("ui.ts", "./util", 1),
        ],
    )
    .unwrap();
    s.upsert_file(&fr("svc.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports("svc.ts", &[ir("svc.ts", "@adapters/storage-idb", 0)])
        .unwrap();

    // deps: modules ui.ts imports, ordered by site.
    let deps = s.deps_of("ui.ts").unwrap();
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].module, "@adapters/storage-idb");
    assert_eq!(deps[1].module, "./util");

    // rdeps: substring match across all importers.
    let importers = s.importers_of("storage-idb").unwrap();
    assert_eq!(importers.len(), 2, "both files import a matching specifier");
    let files: Vec<&str> = importers.iter().map(|e| e.file_path.as_str()).collect();
    assert_eq!(files, ["svc.ts", "ui.ts"], "ordered by file path");

    // No false matches.
    assert!(s.importers_of("nonexistent").unwrap().is_empty());
}

#[test]
fn import_edges_pruned_on_file_reindex() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("a.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports("a.ts", &[ir("a.ts", "gone-mod", 0)])
        .unwrap();
    assert_eq!(s.importers_of("gone-mod").unwrap().len(), 1);
    // Re-index with no imports -> cascade clears the old edge.
    s.upsert_file(&fr("a.ts", 2, 11, "typescript"), &[])
        .unwrap();
    assert_eq!(s.importers_of("gone-mod").unwrap().len(), 0);
}

#[test]
fn rdeps_substring_does_not_match_wildcards_literally() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("a.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports("a.ts", &[ir("a.ts", "@scope/pkg", 0)])
        .unwrap();
    // `%` is escaped, so it is matched literally (no rows), not as a wildcard.
    assert!(s.importers_of("%").unwrap().is_empty());
}

#[test]
fn call_edges_pruned_on_file_reindex() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 10, "rust"),
        &[sr("a.rs", "main", "function", 1)],
    )
    .unwrap();
    s.upsert_calls("a.rs", &[cr("a.rs", "main", 1, "gone", 2)])
        .unwrap();
    assert_eq!(s.callers_of("gone").unwrap().len(), 1);
    // Re-index the file with no calls -> cascade clears the old edge.
    s.upsert_file(
        &fr("a.rs", 2, 11, "rust"),
        &[sr("a.rs", "main", "function", 1)],
    )
    .unwrap();
    assert_eq!(
        s.callers_of("gone").unwrap().len(),
        0,
        "stale edge cascaded away"
    );
}

#[test]
fn hook_events_record_and_aggregate() {
    let mut s = Store::open_in_memory().unwrap();
    s.record_hook_event("rg", "bare-ident", "rewritten")
        .unwrap();
    s.record_hook_event("rg", "bare-ident", "rewritten")
        .unwrap();
    s.record_hook_event("grep", "flagged-nav-ident", "passthrough")
        .unwrap();
    s.record_hook_event("rg", "regex", "passthrough").unwrap();

    let stats = s.hook_event_stats(None).unwrap();
    // Three distinct (idiom, outcome, tool) groups.
    let bare = stats
        .iter()
        .find(|r| r.idiom == "bare-ident" && r.outcome == "rewritten")
        .unwrap();
    assert_eq!(bare.count, 2);
    assert_eq!(bare.tool, "rg");
    assert!(stats
        .iter()
        .any(|r| r.idiom == "flagged-nav-ident" && r.outcome == "passthrough" && r.count == 1));
    // Ordered by count desc -> the 2-count group leads.
    assert_eq!(stats[0].count, 2);
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
