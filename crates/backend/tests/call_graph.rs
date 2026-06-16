//! callers/callees over the TreeSitterBackend (epic af42572 / ADR-0010).

use repoctx_backend::{CodeIntelBackend, TreeSitterBackend};
use repoctx_store::{CallRecord, FileRecord, Store, SymbolRecord};

fn sym(file: &str, name: &str, kind: &str, line: u32) -> SymbolRecord {
    SymbolRecord {
        file_path: file.into(),
        name: name.into(),
        kind: kind.into(),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 1,
        visibility: "unknown".into(),
    }
}

fn call(file: &str, caller: &str, caller_line: u32, callee: &str, site: u32) -> CallRecord {
    CallRecord {
        file_path: file.into(),
        caller_name: caller.into(),
        caller_start_line: caller_line,
        callee_name: callee.into(),
        site_line: site,
        site_column: 4,
        resolution: "syntactic".into(),
        is_method: false,
    }
}

fn backend() -> TreeSitterBackend {
    let mut store = Store::open_in_memory().unwrap();
    // a.rs: main (line 1) calls helper + external_fn.
    store
        .upsert_file(
            &FileRecord {
                path: "a.rs".into(),
                mtime_ns: 1,
                size: 1,
                language: "rust".into(),
            },
            &[
                sym("a.rs", "main", "function", 1),
                sym("a.rs", "helper", "function", 5),
            ],
        )
        .unwrap();
    store
        .upsert_calls(
            "a.rs",
            &[
                call("a.rs", "main", 1, "helper", 2),
                call("a.rs", "main", 1, "external_fn", 3),
            ],
        )
        .unwrap();
    // b.rs: a second `helper` def -> ambiguity.
    store
        .upsert_file(
            &FileRecord {
                path: "b.rs".into(),
                mtime_ns: 1,
                size: 1,
                language: "rust".into(),
            },
            &[sym("b.rs", "helper", "function", 1)],
        )
        .unwrap();
    TreeSitterBackend::new(store)
}

#[test]
fn callers_reports_ambiguous_candidates() {
    let b = backend();
    let edges = b.callers("helper").unwrap();
    assert_eq!(edges.len(), 2, "two helper candidates");
    assert!(edges.iter().all(|e| e.caller.name == "main"));
    assert!(edges.iter().all(|e| e.ambiguous), "helper is ambiguous");
    assert!(edges.iter().all(|e| e.callee.is_some()));
    assert!(edges.iter().all(|e| e.resolution == "syntactic"));
}

#[test]
fn callees_includes_unresolved_external() {
    let b = backend();
    let edges = b.callees("main").unwrap();
    // helper x2 candidates + external_fn (unresolved) = 3 rows.
    assert_eq!(edges.len(), 3);
    let external: Vec<_> = edges.iter().filter(|e| e.callee.is_none()).collect();
    assert_eq!(external.len(), 1);
    assert_eq!(external[0].callee_name, "external_fn");
    assert!(!external[0].ambiguous, "unresolved is not ambiguous");
    assert!(edges
        .iter()
        .any(|e| e.callee_name == "helper" && e.ambiguous));
}

#[test]
fn unknown_symbol_has_no_edges() {
    let b = backend();
    assert!(b.callers("does_not_exist").unwrap().is_empty());
    assert!(b.callees("does_not_exist").unwrap().is_empty());
}
