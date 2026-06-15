//! `repoctx deps <file>` / `repoctx rdeps <module>` — the import /
//! dependency graph (epic #4 / ADR-0011).
//!
//! `deps` lists the module specifiers a file imports; `rdeps` lists the files
//! whose import specifier contains a substring (so `rdeps storage-idb` finds
//! every importer of `@adapters/storage-idb`). String-based, like the call
//! graph — precise specifier→file resolution is deferred (ADR-0011).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use repoctx_store::{ImportEdgeRow, Store};
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, List, Render};
use crate::read_cmd;

/// One import edge for output: the importing `file`, the raw `module`
/// specifier, and the 0-based source line of the import (machine output stays
/// 0-based per the contract; the human renderer shows 1-based).
#[derive(Debug, Clone, Serialize)]
pub struct DepEdge {
    pub file: String,
    pub module: String,
    pub line: u32,
    pub resolution: String,
}

impl From<ImportEdgeRow> for DepEdge {
    fn from(r: ImportEdgeRow) -> Self {
        DepEdge {
            file: r.file_path,
            module: r.module,
            line: r.site_line,
            resolution: r.resolution,
        }
    }
}

impl HumanRender for List<DepEdge> {
    fn human(&self) -> String {
        let mut out = if self.items.is_empty() {
            "no import edges".to_string()
        } else {
            let rows: Vec<(String, String)> = self
                .items
                .iter()
                .map(|e| (format!("{}:{}", e.file, e.line + 1), e.module.clone()))
                .collect();
            let w0 = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
            let mut s = String::new();
            for (i, (loc, module)) in rows.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                s.push_str(&format!("{loc:<w0$}  {module}"));
            }
            s
        };
        if let Some(a) = &self.advisory {
            out.push_str("\n\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}

/// `repoctx deps <file>` — modules `file` imports.
pub fn run_deps(
    repo_root: &Path,
    file_arg: PathBuf,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let db_path = crate::outline_cmd::normalize_path(repo_root, &file_arg)?;
    let store = Store::open(repo_root).context("open store")?;

    let rows = store.deps_of(&db_path)?;
    let advisory = deps_advisory(&rows, &db_path, &store)?;
    let candidate_paths = vec![db_path.clone()];
    let items: Vec<DepEdge> = rows.into_iter().map(DepEdge::from).collect();
    let list = List::new(items).with_advisory(advisory);

    let mut store = store;
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "deps",
        Some(db_path.as_str()),
        &candidate_paths,
    )
}

/// `repoctx rdeps <module>` — files importing a specifier containing `module`.
pub fn run_rdeps(
    repo_root: &Path,
    module: String,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;

    let rows = store.importers_of(&module)?;
    let advisory = rdeps_advisory(&rows, &module);
    let mut candidate_paths: Vec<String> = rows.iter().map(|r| r.file_path.clone()).collect();
    candidate_paths.sort();
    candidate_paths.dedup();
    let items: Vec<DepEdge> = rows.into_iter().map(DepEdge::from).collect();
    let list = List::new(items).with_advisory(advisory);

    let mut store = store;
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "rdeps",
        Some(module.as_str()),
        &candidate_paths,
    )
}

/// `repoctx boundary --from <F> --to <T> [--forbid]` — import edges where an
/// importer path containing `F` imports a specifier containing `T`. Answers
/// "does layer F import T?" structurally (ADR-0011). With `--forbid` it's a
/// CI gate: exit 1 if any crossing exists.
pub fn run_boundary(
    repo_root: &Path,
    from: String,
    to: String,
    forbid: bool,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;

    let rows = store.boundary_crossings(&from, &to)?;
    let crossed = !rows.is_empty();
    let advisory = if rows.is_empty() {
        Some(format!(
            "no crossings: nothing matching `{from}` imports `{to}` (clean — or \
             `{from}`/`{to}` matched no indexed files; core-8 import coverage only)"
        ))
    } else if forbid {
        Some(format!(
            "FORBIDDEN: {} import(s) from `{from}` into `{to}`",
            rows.len()
        ))
    } else {
        None
    };
    let mut candidate_paths: Vec<String> = rows.iter().map(|r| r.file_path.clone()).collect();
    candidate_paths.sort();
    candidate_paths.dedup();
    let items: Vec<DepEdge> = rows.into_iter().map(DepEdge::from).collect();
    let list = List::new(items).with_advisory(advisory);

    let mut store = store;
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "boundary",
        Some(&format!("{from} -> {to}")),
        &candidate_paths,
    )?;

    // CI gate: a forbidden boundary that is crossed fails the command.
    if forbid && crossed {
        std::process::exit(1);
    }
    Ok(())
}

/// Advisory for `deps`: an empty result on a file in an import-uncovered
/// language reads as "no imports" when it really means "not parsed".
fn deps_advisory(rows: &[ImportEdgeRow], file: &str, store: &Store) -> Result<Option<String>> {
    if !rows.is_empty() {
        return Ok(None);
    }
    if !store.file_exists(file).context("file_exists")? {
        return Ok(Some(format!(
            "{file} is not in the index — not on disk, gitignored, oversized, \
             non-UTF-8, or an unsupported language (see `repoctx languages`)"
        )));
    }
    Ok(Some(format!(
        "no import edges for {file} — it may import nothing, or be in a \
         language without import-graph coverage (core 8 only); cross-check \
         with `rg -n \"import|require|use\" {file}`"
    )))
}

/// Advisory for `rdeps`: flag empties so a zero result doesn't read as
/// "definitely unused".
fn rdeps_advisory(rows: &[ImportEdgeRow], module: &str) -> Option<String> {
    if rows.is_empty() {
        return Some(format!(
            "no importers found for `{module}` — nothing imports a matching \
             specifier, or the importers are in a language without import-graph \
             coverage (core 8 only); cross-check with `rg -n {module}`"
        ));
    }
    None
}
