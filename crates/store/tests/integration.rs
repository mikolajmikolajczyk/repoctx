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
    sr_vis(file, name, kind, line, "unknown")
}

fn sr_vis(file: &str, name: &str, kind: &str, line: u32, visibility: &str) -> SymbolRecord {
    SymbolRecord {
        file_path: file.into(),
        name: name.into(),
        kind: kind.into(),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 10,
        visibility: visibility.into(),
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
fn overview_aggregates_modules_entrypoints_hotspots() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("src/main.rs", 1, 100, "rust"),
        &[
            sr("src/main.rs", "main", "function", 0),
            sr("src/main.rs", "helper", "function", 1),
        ],
    )
    .unwrap();
    s.upsert_calls(
        "src/main.rs",
        &[
            cr("src/main.rs", "main", 0, "helper", 0),
            cr("src/main.rs", "main", 0, "helper", 0),
        ],
    )
    .unwrap();

    // file_sizes + symbol_counts_by_file feed module aggregation.
    assert_eq!(
        s.file_sizes().unwrap(),
        vec![("src/main.rs".to_string(), 100)]
    );
    assert_eq!(
        s.symbol_counts_by_file().unwrap(),
        vec![("src/main.rs".to_string(), 2)]
    );

    // entry points: main.
    let ep = s.entry_points().unwrap();
    assert_eq!(ep.len(), 1);
    assert_eq!(ep[0].name, "main");

    // hotspots: helper called twice.
    let hot = s.hotspots(10).unwrap();
    assert_eq!(hot[0].0, "helper");
    assert_eq!(hot[0].1, 2);
    assert_eq!(hot[0].2, "src/main.rs");
}

#[test]
fn hotspots_filters_host_names_and_ambiguous() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.ts", 1, 10, "typescript"),
        &[
            sr("a.ts", "realHotspot", "function", 0),
            sr("a.ts", "get", "method", 1),   // host-method name
            sr("a.ts", "dup", "function", 2), // ambiguous (2nd def below)
        ],
    )
    .unwrap();
    s.upsert_file(
        &fr("b.ts", 1, 10, "typescript"),
        &[sr("b.ts", "dup", "function", 0)],
    )
    .unwrap();
    s.upsert_calls(
        "a.ts",
        &[
            cr("a.ts", "realHotspot", 0, "realHotspot", 0), // 3 calls
            cr("a.ts", "realHotspot", 0, "realHotspot", 0),
            cr("a.ts", "realHotspot", 0, "realHotspot", 0),
            cr("a.ts", "realHotspot", 0, "get", 0), // host name — excluded
            cr("a.ts", "realHotspot", 0, "get", 0),
            cr("a.ts", "realHotspot", 0, "dup", 0), // ambiguous (2 defs) — excluded
            cr("a.ts", "realHotspot", 0, "dup", 0),
        ],
    )
    .unwrap();

    let names: Vec<String> = s.hotspots(10).unwrap().into_iter().map(|h| h.0).collect();
    assert_eq!(names, vec!["realHotspot".to_string()]);
}

#[test]
fn all_import_edges_and_file_paths() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("src/a.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports(
        "src/a.ts",
        &[ir("src/a.ts", "./b", 0), ir("src/a.ts", "react", 1)],
    )
    .unwrap();
    s.upsert_file(&fr("src/b.ts", 1, 10, "typescript"), &[])
        .unwrap();

    let edges = s.all_import_edges().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.contains(&("src/a.ts".to_string(), "./b".to_string())));

    let files = s.all_file_paths().unwrap();
    assert!(files.contains("src/a.ts") && files.contains("src/b.ts"));
    assert_eq!(files.len(), 2);
}

#[test]
fn boundary_crossings_match_from_and_to() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(&fr("src/ui/Panel.tsx", 1, 10, "tsx"), &[])
        .unwrap();
    s.upsert_imports(
        "src/ui/Panel.tsx",
        &[
            ir("src/ui/Panel.tsx", "@adapters/storage-idb", 0),
            ir("src/ui/Panel.tsx", "./local", 1),
        ],
    )
    .unwrap();
    s.upsert_file(&fr("src/core/x.ts", 1, 10, "typescript"), &[])
        .unwrap();
    s.upsert_imports("src/core/x.ts", &[ir("src/core/x.ts", "@adapters/db", 0)])
        .unwrap();

    // ui -> adapters: only the ui file.
    let v = s.boundary_crossings("src/ui", "@adapters").unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].file_path, "src/ui/Panel.tsx");

    // core -> adapters: only the core file.
    assert_eq!(
        s.boundary_crossings("src/core", "@adapters").unwrap().len(),
        1
    );

    // ui -> nonexistent: clean.
    assert!(s.boundary_crossings("src/ui", "@nope").unwrap().is_empty());
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
fn uncalled_symbols_are_dead_code_candidates() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 10, "rust"),
        &[
            sr("a.rs", "main", "function", 0),
            sr("a.rs", "used", "function", 1),
            sr("a.rs", "orphan", "function", 2),
            sr("a.rs", "AStruct", "class", 3), // non-callable kind, never dead
        ],
    )
    .unwrap();
    s.upsert_calls("a.rs", &[cr("a.rs", "main", 0, "used", 0)])
        .unwrap();

    let dead: Vec<String> = s
        .uncalled_symbols(None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    // `used` is called; `main`/`orphan` are not; `AStruct` isn't a fn/method.
    assert!(dead.contains(&"orphan".to_string()));
    assert!(dead.contains(&"main".to_string())); // store doesn't apply entry heuristic
    assert!(!dead.contains(&"used".to_string()));
    assert!(!dead.contains(&"AStruct".to_string()));

    // Language filter.
    assert!(s.uncalled_symbols(Some("python")).unwrap().is_empty());
}

#[test]
fn uncalled_symbols_excludes_tests_and_declarations() {
    let mut s = Store::open_in_memory().unwrap();
    // Uncalled symbols in: a test file, a .d.ts, a tests/ dir, and a real src.
    s.upsert_file(
        &fr("src/foo.test.ts", 1, 10, "typescript"),
        &[sr("src/foo.test.ts", "helperInTest", "function", 0)],
    )
    .unwrap();
    s.upsert_file(
        &fr("src/types.d.ts", 1, 10, "typescript"),
        &[sr("src/types.d.ts", "declaredFn", "function", 0)],
    )
    .unwrap();
    s.upsert_file(
        &fr("tests/contract.ts", 1, 10, "typescript"),
        &[sr("tests/contract.ts", "fixture", "function", 0)],
    )
    .unwrap();
    s.upsert_file(
        &fr("src/real.ts", 1, 10, "typescript"),
        &[sr("src/real.ts", "reallyDead", "function", 0)],
    )
    .unwrap();

    let dead: Vec<String> = s
        .uncalled_symbols(None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(dead, vec!["reallyDead".to_string()], "only the real-src fn");
}

#[test]
fn uncalled_symbols_skips_non_callgraph_langs_and_constructors() {
    let mut s = Store::open_in_memory().unwrap();
    // Bash has no call-site extraction -> every fn would look dead. Excluded.
    s.upsert_file(
        &fr("hook.sh", 1, 10, "bash"),
        &[sr("hook.sh", "print_section", "function", 0)],
    )
    .unwrap();
    // TS constructor: invoked via `new`, never called by name. Excluded.
    s.upsert_file(
        &fr("src/c.ts", 1, 10, "typescript"),
        &[
            sr("src/c.ts", "constructor", "method", 0),
            sr("src/c.ts", "deadMethod", "method", 1),
        ],
    )
    .unwrap();

    let dead: Vec<String> = s
        .uncalled_symbols(None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(dead, vec!["deadMethod".to_string()]);
}

#[test]
fn uncalled_symbols_excludes_public_visibility() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("svc.go", 1, 10, "go"),
        &[
            sr_vis("svc.go", "Exported", "function", 0, "public"),
            sr_vis("svc.go", "internalHelper", "function", 1, "private"),
            sr_vis("svc.go", "noSignal", "function", 2, "unknown"),
        ],
    )
    .unwrap();
    let dead: Vec<String> = s
        .uncalled_symbols(None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    // public excluded; private + unknown still flagged (unknown = keep prior behavior).
    assert!(
        !dead.contains(&"Exported".to_string()),
        "public is API, not dead"
    );
    assert!(dead.contains(&"internalHelper".to_string()));
    assert!(dead.contains(&"noSignal".to_string()));
}

#[test]
fn resolved_edge_pairs_only_in_repo() {
    let mut s = Store::open_in_memory().unwrap();
    s.upsert_file(
        &fr("a.rs", 1, 10, "rust"),
        &[
            sr("a.rs", "a", "function", 0),
            sr("a.rs", "b", "function", 1),
        ],
    )
    .unwrap();
    s.upsert_calls(
        "a.rs",
        &[
            cr("a.rs", "a", 0, "b", 0),        // resolved (b is a symbol)
            cr("a.rs", "b", 1, "a", 1),        // resolved (cycle)
            cr("a.rs", "a", 0, "external", 0), // unresolved -> excluded
        ],
    )
    .unwrap();
    let pairs = s.resolved_edge_pairs().unwrap();
    assert!(pairs.contains(&("a".to_string(), "b".to_string())));
    assert!(pairs.contains(&("b".to_string(), "a".to_string())));
    assert!(!pairs.iter().any(|(_, c)| c == "external"));
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
fn hook_samples_capped_per_idiom() {
    let mut s = Store::open_in_memory().unwrap();
    // Insert 5 "other" samples with a cap of 3 -> only the latest 3 survive.
    for i in 0..5 {
        s.record_hook_sample("rg", "other", "chained", &format!("rg cmd{i}"), 3)
            .unwrap();
    }
    s.record_hook_sample("grep", "regex", "chained", "grep foo.*", 3)
        .unwrap();

    let other = s.hook_samples(Some("other")).unwrap();
    assert_eq!(other.len(), 3, "capped at 3 per idiom");
    // Newest first: cmd4, cmd3, cmd2.
    assert_eq!(other[0].command, "rg cmd4");
    assert_eq!(other[2].command, "rg cmd2");

    // Filter by idiom + unfiltered.
    assert_eq!(s.hook_samples(Some("regex")).unwrap().len(), 1);
    assert_eq!(s.hook_samples(None).unwrap().len(), 4);
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
