//! `repoctx index` — walk, parse changed files in parallel, write through
//! a single sequential SQLite writer, prune absent files, report a summary.

use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, Result};
use rayon::prelude::*;
use repoctx_index::{parse_calls_with, parse_file_with, ParseOptions};
use repoctx_store::{CallRecord, FileRecord, Store, SymbolRecord};
use serde::Serialize;
use tracing::{debug, warn};

use crate::output::{HumanRender, Render};
use crate::walk::{collect_indexable, collect_quiet, Candidate};

#[derive(Debug, Default, Clone, Serialize)]
pub struct IndexSummary {
    pub indexed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub duration_ms: u128,
}

impl HumanRender for IndexSummary {
    fn human(&self) -> String {
        format!(
            "indexed {} files ({} unchanged, {} removed) in {} ms",
            self.indexed, self.unchanged, self.removed, self.duration_ms
        )
    }
}

pub fn run(repo_root: &Path, force: bool, render: Render) -> Result<()> {
    let summary = do_index(repo_root, force, true)?;
    crate::output::emit(&summary, render)
}

/// Same as `run` but returns the summary instead of rendering it, and
/// suppresses the skip warnings. Used by `read_cmd::ensure_fresh` so an
/// auto-reindex that fires during a read command doesn't litter stdout
/// or re-emit the same skip warning on every call.
pub fn run_silent(repo_root: &Path, force: bool) -> Result<IndexSummary> {
    do_index(repo_root, force, false)
}

pub(crate) fn do_index(repo_root: &Path, force: bool, warn_on_skip: bool) -> Result<IndexSummary> {
    let started = Instant::now();
    let mut store = Store::open(repo_root).context("open store")?;
    let existing = store.file_mtimes().context("read mtimes")?;

    let candidates = if warn_on_skip {
        collect_indexable(repo_root)?
    } else {
        collect_quiet(repo_root)?
    };

    let mut on_disk: HashSet<String> = HashSet::with_capacity(candidates.len());
    let mut to_parse: Vec<Candidate> = Vec::new();
    let mut unchanged = 0usize;
    for c in candidates {
        on_disk.insert(c.rel.clone());
        let dirty = force
            || match existing.get(&c.rel) {
                Some(&(m, s)) => m != c.mtime_ns || s != c.size,
                None => true,
            };
        if dirty {
            to_parse.push(c);
        } else {
            unchanged += 1;
        }
    }

    // Opt-in nested-key extraction for JSON/YAML/TOML (issue 2c47040),
    // read from the per-repo settings table. Flipping it requires a
    // `repoctx index --force` to re-parse existing files.
    let parse_opts = ParseOptions {
        nested_keys: store
            .get_setting("index.nested_keys")
            .ok()
            .flatten()
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes"))
            .unwrap_or(false),
    };

    let (tx, rx) = mpsc::sync_channel::<(FileRecord, Vec<SymbolRecord>, Vec<CallRecord>)>(64);
    let parse_handle = std::thread::spawn(move || {
        to_parse.into_par_iter().for_each_with(tx, |tx, c| {
            let source = match std::fs::read_to_string(&c.abs) {
                Ok(s) => s,
                Err(e) => {
                    debug!(path = %c.abs.display(), error = %e, "read failed");
                    return;
                }
            };
            let symbols = match parse_file_with(&c.rel, c.language, &source, parse_opts) {
                Ok(v) => v,
                Err(e) => {
                    debug!(path = %c.abs.display(), error = %e, "parse failed");
                    Vec::new()
                }
            };
            // Call edges (static call graph, ADR-0010). No-op for languages
            // without a call query; never blocks the file's symbol indexing.
            let calls = match parse_calls_with(&c.rel, c.language, &source, &symbols) {
                Ok(v) => v,
                Err(e) => {
                    debug!(path = %c.abs.display(), error = %e, "call extraction failed");
                    Vec::new()
                }
            };
            let record = FileRecord {
                path: c.rel,
                mtime_ns: c.mtime_ns,
                size: c.size,
                language: c.language.slug().to_string(),
            };
            if tx.send((record, symbols, calls)).is_err() {
                // Writer hung up (receiver dropped) — nothing more to do.
                debug!(path = %c.abs.display(), "index: writer closed; dropping parse result");
            }
        });
    });

    let mut indexed = 0usize;
    while let Ok((file, symbols, calls)) = rx.recv() {
        if let Err(e) = store.upsert_file(&file, &symbols) {
            warn!(path = %file.path, error = %e, "upsert failed");
            continue;
        }
        if let Err(e) = store.upsert_calls(&file.path, &calls) {
            warn!(path = %file.path, error = %e, "call upsert failed");
        }
        indexed += 1;
    }
    // Surface a panic in the parse pool instead of swallowing it.
    if parse_handle.join().is_err() {
        anyhow::bail!("index: parser thread panicked");
    }

    let absent: Vec<String> = existing
        .keys()
        .filter(|k| !on_disk.contains(*k))
        .cloned()
        .collect();
    let removed = store.prune(&absent).context("prune absent")?;

    Ok(IndexSummary {
        indexed,
        unchanged,
        removed,
        duration_ms: started.elapsed().as_millis(),
    })
}
