//! `repoctx symbols` — substring search backed by `TreeSitterBackend`.

use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use repoctx_backend::{CodeIntelBackend, SymbolKind, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;

use crate::gain::{GainOpts, Recorder};
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
    let symbols = backend.workspace_symbols(&q)?;

    let candidate_paths = unique_paths(&symbols);
    let list = List::new(symbols);

    let mut buf = Vec::new();
    crate::output::emit_to(&mut buf, &list, render)?;
    std::io::Write::write_all(&mut std::io::stdout().lock(), &buf)?;

    let rendered = String::from_utf8_lossy(&buf).into_owned();
    let mut store = backend.into_store();
    let mut recorder = Recorder::new(&mut store, gain_opts);
    recorder.record(
        "symbols",
        Some(query.as_str()),
        &candidate_paths,
        &rendered,
        render.name(),
    );
    Ok(())
}

fn unique_paths(symbols: &[repoctx_backend::Symbol]) -> Vec<String> {
    let mut out: Vec<String> = symbols.iter().map(|s| s.location.path.clone()).collect();
    out.sort();
    out.dedup();
    out
}
