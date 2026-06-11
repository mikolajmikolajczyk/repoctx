//! Record types crossing the store boundary.
//!
//! Positions are stored as Tree-sitter native 0-based line and column.
//! Paths are repo-root-relative with `/` separators (DB convention),
//! produced from filesystem paths via [`to_db_path`] / [`from_db_path`].

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub path: String,
    pub mtime_ns: i64,
    pub size: i64,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Native filesystem path -> DB path string (`/`-separated, lossy on non-UTF-8 components).
pub fn to_db_path(p: &Path) -> String {
    let mut out = String::new();
    for (i, comp) in p.components().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&comp.as_os_str().to_string_lossy());
    }
    out
}

/// DB path string -> native filesystem path (splits on `/`).
pub fn from_db_path(p: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for seg in p.split('/').filter(|s| !s.is_empty()) {
        out.push(seg);
    }
    out
}
