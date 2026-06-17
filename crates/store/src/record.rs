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
    /// Lexical visibility (issue #10): `"public"` | `"private"` | `"unknown"`.
    /// Set per-language by the extractor; `"unknown"` where the language has
    /// no visibility signal yet (safe default — keeps prior behavior).
    pub visibility: String,
}

/// One call SITE for insertion (static call graph, epic af42572 / ADR-0010).
///
/// Stored name-based + caller-located, never by symbol id (ids churn on
/// reindex). Callee resolution happens at query time. `resolution` is
/// 'syntactic' for Tree-sitter edges, 'semantic' for a future LSP backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    pub file_path: String,
    pub caller_name: String,
    pub caller_start_line: u32,
    pub callee_name: String,
    pub site_line: u32,
    pub site_column: u32,
    pub resolution: String,
    /// Receiver-value method call (`obj.foo()`) vs free/path call (#9). A
    /// method call must not resolve to a free `function` of the same name.
    pub is_method: bool,
}

/// A resolved call edge returned by a callers/callees query. The caller is
/// always a concrete repo symbol (joined on file + name + start line); the
/// callee is `Some` when `callee_name` resolved to a repo symbol and `None`
/// when it is external/unresolved. Ambiguity (a name resolving to several
/// symbols) surfaces as multiple rows sharing one call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEdgeRow {
    pub caller: SymbolRecord,
    pub callee_name: String,
    pub callee: Option<SymbolRecord>,
    pub site_line: u32,
    pub site_column: u32,
    pub resolution: String,
}

/// A call edge with both endpoints located by definition (issue: node
/// identity). The caller is identified by `(name, file, start_line)` — its
/// enclosing definition — so distinct same-named definitions stay distinct
/// graph nodes instead of collapsing into one fake super-hub. The callee
/// carries its def-count: `1` = resolved (location in `callee_file`/`line`),
/// `>1` = ambiguous (location unknown — `callee_file`/`line` are `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedEdge {
    pub caller_name: String,
    pub caller_file: String,
    pub caller_line: u32,
    pub callee_name: String,
    pub callee_defs: usize,
    /// `Some` only when `callee_defs == 1`.
    pub callee_file: Option<String>,
    /// `Some` only when `callee_defs == 1`.
    pub callee_line: Option<u32>,
}

/// One import SITE for insertion (import graph, epic #4 / ADR-0011).
///
/// `module` is the raw specifier as written in source (quotes/brackets
/// already stripped). String-based, resolved at query time; `resolution` is
/// 'syntactic' for Tree-sitter edges, 'semantic' for a future resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    pub file_path: String,
    pub module: String,
    pub site_line: u32,
    pub site_column: u32,
    pub resolution: String,
}

/// An import edge returned by a `deps`/`rdeps` query: the importing file, the
/// raw module specifier, and where the import sits. Both directions share
/// this shape (no symbol resolution, unlike call edges).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdgeRow {
    pub file_path: String,
    pub module: String,
    pub site_line: u32,
    pub site_column: u32,
    pub resolution: String,
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
