//! `repoctx definition <name>` — exact-name symbol lookup filtered to a
//! kind whitelist suitable for "where is X defined" answers.

use std::path::Path;

use anyhow::{Context, Result};
use repoctx_backend::{CodeIntelBackend, Symbol, SymbolKind, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;

use crate::gain::{GainOpts, Recorder};
use crate::output::{List, Render};
use crate::read_cmd;

/// Per issue be537dc: "function, method, class, interface, type, module,
/// macro, constant". Rust struct/enum/trait reach this set already via
/// upstream `tags.scm` mapping to `class` / `interface` / `type`.
const KIND_WHITELIST: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Type,
    SymbolKind::Module,
    SymbolKind::Macro,
    SymbolKind::Constant,
];

fn is_definition_kind(k: SymbolKind) -> bool {
    KIND_WHITELIST.contains(&k)
}

pub fn run(
    repo_root: &Path,
    name: String,
    lang: Option<String>,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);
    let q = SymbolQuery {
        query: name.clone(),
        kind: None,
        language: lang,
        limit: 0, // unlimited; CLI filters + truncates
    };
    let mut hits = backend.workspace_symbols(&q)?;
    hits.retain(|s| s.name == name && is_definition_kind(s.kind));
    hits.truncate(limit);

    let candidate_paths = unique_paths(&hits);
    let list = List::new(hits);

    let mut buf = Vec::new();
    crate::output::emit_to(&mut buf, &list, render)?;
    std::io::Write::write_all(&mut std::io::stdout().lock(), &buf)?;

    let rendered = String::from_utf8_lossy(&buf).into_owned();
    let mut store = backend.into_store();
    let mut recorder = Recorder::new(&mut store, gain_opts);
    recorder.record(
        "definition",
        Some(name.as_str()),
        &candidate_paths,
        &rendered,
        render.name(),
    );
    Ok(())
}

fn unique_paths(symbols: &[Symbol]) -> Vec<String> {
    let mut out: Vec<String> = symbols.iter().map(|s| s.location.path.clone()).collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_excludes_variable_and_field() {
        assert!(!is_definition_kind(SymbolKind::Variable));
        assert!(!is_definition_kind(SymbolKind::Field));
        assert!(!is_definition_kind(SymbolKind::Section));
        assert!(!is_definition_kind(SymbolKind::Key));
        assert!(!is_definition_kind(SymbolKind::Other));
    }

    #[test]
    fn whitelist_includes_callables_and_types() {
        for k in [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Interface,
            SymbolKind::Type,
            SymbolKind::Module,
            SymbolKind::Macro,
            SymbolKind::Constant,
        ] {
            assert!(is_definition_kind(k), "{k:?} should be in whitelist");
        }
    }
}
