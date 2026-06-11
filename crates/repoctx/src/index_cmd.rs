//! `repoctx index` — walk, parse changed files in parallel, write through
//! a single sequential SQLite writer, prune absent files, report a summary.

use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, Result};
use rayon::prelude::*;
use repoctx_index::parse_file;
use repoctx_store::{FileRecord, Store, SymbolRecord};
use serde::Serialize;
use tracing::{debug, warn};

use crate::output::{HumanRender, Render};
use crate::walk::{collect_indexable, Candidate};

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
    let started = Instant::now();
    let mut store = Store::open(repo_root).context("open store")?;
    let existing = store.file_mtimes().context("read mtimes")?;

    let candidates = collect_indexable(repo_root)?;

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

    let (tx, rx) = mpsc::sync_channel::<(FileRecord, Vec<SymbolRecord>)>(64);
    let parse_handle = std::thread::spawn(move || {
        to_parse.into_par_iter().for_each_with(tx, |tx, c| {
            let source = match std::fs::read_to_string(&c.abs) {
                Ok(s) => s,
                Err(e) => {
                    debug!(path = %c.abs.display(), error = %e, "read failed");
                    return;
                }
            };
            let symbols = match parse_file(&c.rel, c.language, &source) {
                Ok(v) => v,
                Err(e) => {
                    debug!(path = %c.abs.display(), error = %e, "parse failed");
                    Vec::new()
                }
            };
            let record = FileRecord {
                path: c.rel,
                mtime_ns: c.mtime_ns,
                size: c.size,
                language: c.language.slug().to_string(),
            };
            let _ = tx.send((record, symbols));
        });
    });

    let mut indexed = 0usize;
    while let Ok((file, symbols)) = rx.recv() {
        if let Err(e) = store.upsert_file(&file, &symbols) {
            warn!(path = %file.path, error = %e, "upsert failed");
            continue;
        }
        indexed += 1;
    }
    parse_handle.join().ok();

    let absent: Vec<String> = existing
        .keys()
        .filter(|k| !on_disk.contains(*k))
        .cloned()
        .collect();
    let removed = store.prune(&absent).context("prune absent")?;

    let summary = IndexSummary {
        indexed,
        unchanged,
        removed,
        duration_ms: started.elapsed().as_millis(),
    };

    crate::output::emit(&summary, render)
}
