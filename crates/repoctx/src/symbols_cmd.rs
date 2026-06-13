//! `repoctx symbols` — substring search backed by `TreeSitterBackend`.

use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use repoctx_backend::{CodeIntelBackend, SymbolKind, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;

use crate::gain::GainOpts;
use crate::output::{List, Render};
use crate::read_cmd;

#[allow(clippy::too_many_arguments)]
pub fn run(
    repo_root: &Path,
    query: String,
    kind: Option<String>,
    lang: Option<String>,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let kind_enum = match &kind {
        None => None,
        Some(s) => Some(SymbolKind::from_str(s).map_err(|e| anyhow!("{}", e))?),
    };
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);
    let q = SymbolQuery {
        query: query.clone(),
        kind: kind_enum,
        language: lang,
        limit,
    };
    let lang_filter = q.language.clone();
    let symbols = backend.workspace_symbols(&q)?;

    let candidate_paths = unique_paths(&symbols);
    let advisory = crate::definition_cmd::compute_advisory(
        &backend,
        lang_filter.as_deref(),
        &query,
        symbols.len(),
    )?;
    let list = List::new(symbols).with_advisory(advisory);

    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "symbols",
        Some(query.as_str()),
        &candidate_paths,
    )
}

fn unique_paths(symbols: &[repoctx_backend::Symbol]) -> Vec<String> {
    let mut out: Vec<String> = symbols.iter().map(|s| s.location.path.clone()).collect();
    out.sort();
    out.dedup();
    out
}
