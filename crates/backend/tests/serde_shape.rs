//! Public output contract per ADR-0008: field names + kind vocabulary are
//! stable. Renames/removals are breaking — this file is the tripwire.

use std::path::PathBuf;

use repoctx_backend::{HoverInfo, Location, PositionQuery, Symbol, SymbolKind, SymbolQuery};

fn json(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn symbol_kind_serializes_lowercase() {
    assert_eq!(json(&SymbolKind::Function), r#""function""#);
    assert_eq!(json(&SymbolKind::Method), r#""method""#);
    assert_eq!(json(&SymbolKind::Class), r#""class""#);
    assert_eq!(json(&SymbolKind::Struct), r#""struct""#);
    assert_eq!(json(&SymbolKind::Enum), r#""enum""#);
    assert_eq!(json(&SymbolKind::Interface), r#""interface""#);
    assert_eq!(json(&SymbolKind::Trait), r#""trait""#);
    assert_eq!(json(&SymbolKind::Module), r#""module""#);
    assert_eq!(json(&SymbolKind::Constant), r#""constant""#);
    assert_eq!(json(&SymbolKind::Type), r#""type""#);
    assert_eq!(json(&SymbolKind::Variable), r#""variable""#);
    assert_eq!(json(&SymbolKind::Field), r#""field""#);
    assert_eq!(json(&SymbolKind::Macro), r#""macro""#);
    assert_eq!(json(&SymbolKind::Section), r#""section""#);
    assert_eq!(json(&SymbolKind::Key), r#""key""#);
    assert_eq!(json(&SymbolKind::Other), r#""other""#);
}

#[test]
fn location_field_names_are_stable() {
    let loc = Location {
        path: "src/main.rs".into(),
        start_line: 0,
        start_column: 0,
        end_line: 0,
        end_column: 7,
    };
    assert_eq!(
        json(&loc),
        r#"{"path":"src/main.rs","start_line":0,"start_column":0,"end_line":0,"end_column":7}"#
    );
}

#[test]
fn symbol_shape() {
    let s = Symbol {
        name: "foo".into(),
        kind: SymbolKind::Function,
        location: Location {
            path: "a.rs".into(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 3,
        },
    };
    assert_eq!(
        json(&s),
        r#"{"name":"foo","kind":"function","location":{"path":"a.rs","start_line":1,"start_column":0,"end_line":1,"end_column":3}}"#
    );
}

#[test]
fn hover_shape() {
    let h = HoverInfo {
        contents: "docs".into(),
    };
    assert_eq!(json(&h), r#"{"contents":"docs"}"#);
}

#[test]
fn symbol_query_skips_none_filters() {
    let q = SymbolQuery {
        query: "foo".into(),
        kind: None,
        language: None,
        limit: 50,
    };
    assert_eq!(json(&q), r#"{"query":"foo","limit":50}"#);

    let q = SymbolQuery {
        query: "foo".into(),
        kind: Some(SymbolKind::Function),
        language: Some("rust".into()),
        limit: 10,
    };
    assert_eq!(
        json(&q),
        r#"{"query":"foo","kind":"function","language":"rust","limit":10}"#
    );
}

#[test]
fn position_query_shape() {
    let q = PositionQuery {
        path: PathBuf::from("src/main.rs"),
        line: 3,
        column: 7,
    };
    assert_eq!(json(&q), r#"{"path":"src/main.rs","line":3,"column":7}"#);
}

#[test]
fn symbol_kind_round_trips() {
    let kinds = [
        SymbolKind::Function,
        SymbolKind::Method,
        SymbolKind::Class,
        SymbolKind::Struct,
        SymbolKind::Enum,
        SymbolKind::Interface,
        SymbolKind::Trait,
        SymbolKind::Module,
        SymbolKind::Constant,
        SymbolKind::Type,
        SymbolKind::Variable,
        SymbolKind::Field,
        SymbolKind::Macro,
        SymbolKind::Section,
        SymbolKind::Key,
        SymbolKind::Other,
    ];
    for k in kinds {
        let s = json(&k);
        let back: SymbolKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, k);
    }
}
