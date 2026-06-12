//! `repoctx-index` — Tree-sitter parsing + symbol extraction.
//!
//! Produces `repoctx-store` record types. Must NOT depend on `repoctx-backend`.
//! ADR-0002.

mod extractor;
mod language;

pub use extractor::{parse_file, ExtractError};
pub use language::{Coverage, Language, ALL_LANGUAGES};
