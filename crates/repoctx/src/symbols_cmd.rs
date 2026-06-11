//! `repoctx symbols` — substring search backed by `TreeSitterBackend`.

use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use repoctx_backend::{CodeIntelBackend, SymbolKind, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;

use crate::output::{List, Render};
use crate::read_cmd;

pub fn run(
    repo_root: &Path,
    query: String,
    kind: Option<String>,
    lang: Option<String>,
    limit: usize,
    render: Render,
) -> Result<()> {
    read_cmd::ensure_indexed(repo_root)?;
    let kind = match kind {
        None => None,
        Some(s) => Some(SymbolKind::from_str(&s).map_err(|e| anyhow!("{}", e))?),
    };
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);
    let q = SymbolQuery {
        query,
        kind,
        language: lang,
        limit,
    };
    let symbols = backend.workspace_symbols(&q)?;
    let list = List::new(symbols);
    crate::output::emit(&list, render)
}
