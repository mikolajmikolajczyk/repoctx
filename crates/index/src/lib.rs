//! `repoctx-index` — Tree-sitter parsing + symbol extraction.
//!
//! Produces `repoctx-store` record types. Must NOT depend on `repoctx-backend`.
//! ADR-0002.

mod extractor;
mod language;

pub use extractor::{parse_calls_with, parse_file, parse_file_with, ExtractError, ParseOptions};
pub use language::{Coverage, Language, ALL_LANGUAGES};
