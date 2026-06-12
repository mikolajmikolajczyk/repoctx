//! Tree-sitter-backed implementation of [`CodeIntelBackend`] reading from
//! the SQLite store. M0 default backend per ADR-0002. Position-based
//! capabilities (`definition`, `references`, `hover`) require LSP and are
//! reported as `Unsupported` here (ADR-0005).

use std::path::Path;
use std::str::FromStr;

use repoctx_store::{Store, SymbolFilter};

use crate::error::{BackendError, Result};
use crate::kind::SymbolKind;
use crate::trait_def::CodeIntelBackend;
use crate::types::{HoverInfo, Location, PositionQuery, Symbol, SymbolQuery};

pub struct TreeSitterBackend {
    store: Store,
}

impl TreeSitterBackend {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Take ownership of the underlying store back. Useful when a command
    /// needs the store for follow-on work (e.g. gain recording) after the
    /// query is done.
    pub fn into_store(self) -> Store {
        self.store
    }

    /// Borrow the underlying store. Useful when a command needs a read
    /// (e.g. workspace per-language counts for the advisory layer) but
    /// still wants to keep the backend around for follow-on queries.
    pub fn store(&self) -> &Store {
        &self.store
    }
}

impl CodeIntelBackend for TreeSitterBackend {
    fn workspace_symbols(&self, query: &SymbolQuery) -> Result<Vec<Symbol>> {
        let filter = SymbolFilter {
            kind: query.kind.as_ref().map(|k| k.as_str()),
            language: query.language.as_deref(),
            limit: if query.limit == 0 {
                None
            } else {
                Some(query.limit)
            },
        };
        let rows = self.store.symbols_substring(&query.query, &filter)?;
        rows.into_iter().map(|(r, _lang)| to_symbol(r)).collect()
    }

    fn document_symbols(&self, file: &Path) -> Result<Vec<Symbol>> {
        let path = repoctx_store::to_db_path(file);
        let rows = self.store.symbols_by_file(&path)?;
        rows.into_iter().map(to_symbol).collect()
    }

    fn definition(&self, _: &PositionQuery) -> Result<Vec<Location>> {
        Err(BackendError::Unsupported {
            capability: "definition (position-based; requires LSP)",
        })
    }

    fn references(&self, _: &PositionQuery) -> Result<Vec<Location>> {
        Err(BackendError::Unsupported {
            capability: "references",
        })
    }

    fn hover(&self, _: &PositionQuery) -> Result<Option<HoverInfo>> {
        Err(BackendError::Unsupported {
            capability: "hover",
        })
    }
}

fn to_symbol(r: repoctx_store::SymbolRecord) -> Result<Symbol> {
    let kind = SymbolKind::from_str(&r.kind).unwrap_or(SymbolKind::Other);
    Ok(Symbol {
        name: r.name,
        kind,
        location: Location {
            path: r.file_path,
            start_line: r.start_line,
            start_column: r.start_column,
            end_line: r.end_line,
            end_column: r.end_column,
        },
    })
}
