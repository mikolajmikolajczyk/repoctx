use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::kind::SymbolKind;

/// Source-text range, repo-root-relative path, 0-based positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    pub contents: String,
}

/// A static call-graph edge (epic af42572 / ADR-0010). Name-based: the
/// `callee` is resolved by name and may be `None` (external/unresolved) or
/// one of several candidates when `ambiguous`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEdge {
    /// Enclosing function/method the call is made from.
    pub caller: Symbol,
    /// Callee name as written at the call site.
    pub callee_name: String,
    /// Resolved callee symbol, or `None` when external/unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee: Option<Symbol>,
    /// Call-site location (in the caller's file).
    pub site: Location,
    /// `"syntactic"` (Tree-sitter) or `"semantic"` (LSP, future).
    pub resolution: String,
    /// True when `callee_name` resolves to more than one repo symbol.
    pub ambiguous: bool,
}

/// Workspace-wide search input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SymbolQuery {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SymbolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub limit: usize,
}

/// Position-based input. Used by `definition`/`references`/`hover` (LSP path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionQuery {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}
