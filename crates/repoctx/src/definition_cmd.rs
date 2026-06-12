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
    let lang_filter = q.language.clone();
    let mut hits = backend.workspace_symbols(&q)?;
    // `workspace_symbols` is case-insensitive substring; capture exact-case
    // near-misses (same name modulo ASCII case, definition-shaped) before
    // the exact-case retain drops them, so a 0-hit can advise instead of
    // looking like "doesn't exist".
    let case_candidates = case_near_misses(&hits, &name);
    hits.retain(|s| s.name == name && is_definition_kind(s.kind));
    hits.truncate(limit);

    let candidate_paths = unique_paths(&hits);
    let advisory = if hits.is_empty() {
        match crate::advisory::for_case_mismatch(&name, &case_candidates) {
            Some(a) => Some(a),
            None => compute_advisory(&backend, lang_filter.as_deref(), &name, hits.len())?,
        }
    } else {
        compute_advisory(&backend, lang_filter.as_deref(), &name, hits.len())?
    };
    let list = List::new(hits).with_advisory(advisory);

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

/// From the unfiltered (case-insensitive substring) hit set, pick exact
/// case-insensitive name matches of a definition kind, deduped by name,
/// capped at 3. Excludes exact-case equals (those are real hits).
fn case_near_misses(hits: &[Symbol], name: &str) -> Vec<crate::advisory::CaseCandidate> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for s in hits {
        if s.name != name
            && s.name.eq_ignore_ascii_case(name)
            && is_definition_kind(s.kind)
            && !seen.contains(&s.name)
        {
            seen.push(s.name.clone());
            out.push(crate::advisory::CaseCandidate {
                name: s.name.clone(),
                path: s.location.path.clone(),
                line: s.location.start_line + 1,
            });
            if out.len() == 3 {
                break;
            }
        }
    }
    out
}

fn unique_paths(symbols: &[Symbol]) -> Vec<String> {
    let mut out: Vec<String> = symbols.iter().map(|s| s.location.path.clone()).collect();
    out.sort();
    out.dedup();
    out
}

/// Pick the most specific advisory for a `definition` / `context` /
/// `symbols`-style query:
///
/// 1. `--lang` was set and that language has partial coverage → advise.
/// 2. Otherwise, zero hits + workspace has partial-coverage files →
///    advise.
/// 3. Otherwise → no advisory.
pub(crate) fn compute_advisory(
    backend: &TreeSitterBackend,
    lang_filter: Option<&str>,
    query: &str,
    hit_count: usize,
) -> Result<Option<String>> {
    if let Some(a) = crate::advisory::for_lang_filter(lang_filter, Some(query)) {
        return Ok(Some(a));
    }
    if hit_count == 0 {
        let counts = backend.store().counts().context("counts")?;
        return Ok(crate::advisory::for_empty_workspace(
            hit_count,
            &counts.per_language,
            Some(query),
        ));
    }
    Ok(None)
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
