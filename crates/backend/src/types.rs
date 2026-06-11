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
