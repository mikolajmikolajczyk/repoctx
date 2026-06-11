//! `repoctx-store` — SQLite source of truth for indexed file + symbol metadata.
//!
//! Layer: bottom. No internal repoctx deps. See `wiki/agents/architecture.md`
//! and ADR-0003 / ADR-0006 / ADR-0007.
//!
//! Position convention: 0-based line and column everywhere in this crate
//! (Tree-sitter native). Renderers may display 1-based lines for humans.

mod error;
mod like;
mod migrations;
mod record;
mod store;

pub use error::{Result, StoreError};
pub use migrations::SUPPORTED_VERSION;
pub use record::{from_db_path, to_db_path, FileRecord, SymbolRecord};
pub use store::{Counts, Store, SymbolFilter};
