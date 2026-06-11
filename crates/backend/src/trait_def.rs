use std::path::Path;

use crate::error::Result;
use crate::types::{HoverInfo, Location, PositionQuery, Symbol, SymbolQuery};

/// Code-intelligence backend (ADR-0004). The CLI talks only to this trait.
///
/// Implementations:
/// - `TreeSitterBackend` (M0): `workspace_symbols`, `document_symbols`.
///   Position-based methods return `Unsupported` — semantic resolution
///   needs LSP (ADR-0005).
/// - `LspBackend` (M2, future): adds position-based methods via the
///   `repoctxd` daemon.
pub trait CodeIntelBackend {
    fn workspace_symbols(&self, query: &SymbolQuery) -> Result<Vec<Symbol>>;
    fn document_symbols(&self, file: &Path) -> Result<Vec<Symbol>>;
    fn definition(&self, query: &PositionQuery) -> Result<Vec<Location>>;
    fn references(&self, query: &PositionQuery) -> Result<Vec<Location>>;
    fn hover(&self, query: &PositionQuery) -> Result<Option<HoverInfo>>;
}
