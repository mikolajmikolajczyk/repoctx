//! `repoctx-backend` — the `CodeIntelBackend` trait + public output types.
//!
//! Layer: CLI talks only to this trait. Reads from `repoctx-store`. ADR-0004.
//!
//! Field names on `Serialize` types are the public output contract (ADR-0008):
//! additive changes are allowed, renames/removals are breaking.

mod error;
mod kind;
mod trait_def;
mod types;

pub use error::{BackendError, Result};
pub use kind::{SymbolKind, UnknownKindError};
pub use trait_def::CodeIntelBackend;
pub use types::{HoverInfo, Location, PositionQuery, Symbol, SymbolQuery};
