//! Format snapshot tests. ADR-0008 contract.

#![cfg(test)]

use repoctx_backend::{Location, Symbol, SymbolKind};

use crate::output::{emit_to, List, Render};

fn fixture() -> List<Symbol> {
    List::new(vec![
        Symbol {
            name: "main".into(),
            kind: SymbolKind::Function,
            location: Location {
                path: "src/main.rs".into(),
                start_line: 0,
                start_column: 0,
                end_line: 0,
                end_column: 4,
            },
        },
        Symbol {
            name: "MyType".into(),
            kind: SymbolKind::Class,
            location: Location {
                path: "src/lib.rs".into(),
                start_line: 9,
                start_column: 0,
                end_line: 9,
                end_column: 6,
            },
        },
    ])
}

fn render(r: Render) -> String {
    let mut buf = Vec::new();
    emit_to(&mut buf, &fixture(), r).unwrap();
    String::from_utf8(buf).unwrap()
}

#[test]
fn json_shape_is_compact_with_trailing_newline() {
    assert_eq!(
        render(Render::Json),
        concat!(
            r#"{"count":2,"items":["#,
            r#"{"name":"main","kind":"function","location":{"path":"src/main.rs","start_line":0,"start_column":0,"end_line":0,"end_column":4}},"#,
            r#"{"name":"MyType","kind":"class","location":{"path":"src/lib.rs","start_line":9,"start_column":0,"end_line":9,"end_column":6}}"#,
            "]}\n",
        )
    );
}

#[test]
fn human_is_aligned_columns_one_based_line() {
    let s = render(Render::Human);
    assert_eq!(
        s,
        "src/main.rs:1  main    function\nsrc/lib.rs:10  MyType  class\n",
    );
}

#[test]
fn human_empty_list() {
    let mut buf = Vec::new();
    emit_to(&mut buf, &List::<Symbol>::new(vec![]), Render::Human).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), "no symbols\n");
}

#[test]
fn toon_renders_without_panic_and_ends_with_newline() {
    let s = render(Render::Toon);
    assert!(
        s.ends_with('\n'),
        "toon output should end with newline: {s:?}"
    );
    assert!(
        s.contains("main"),
        "toon output should mention 'main': {s:?}"
    );
    assert!(
        s.contains("MyType"),
        "toon output should mention 'MyType': {s:?}"
    );
}
