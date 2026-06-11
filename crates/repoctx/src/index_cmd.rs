//! `repoctx index` — walk, parse changed files in parallel, write through
//! a single sequential SQLite writer, prune absent files, report a summary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;
use repoctx_index::{parse_file, Language};
use repoctx_store::{FileRecord, Store, SymbolRecord};
use serde::Serialize;
use tracing::{debug, warn};

use crate::output::{HumanRender, Render};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

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

#[derive(Debug)]
struct Candidate {
    abs: PathBuf,
    rel: String,
    language: Language,
    mtime_ns: i64,
    size: i64,
}

pub fn run(repo_root: &Path, force: bool, render: Render) -> Result<()> {
    let started = Instant::now();
    let mut store = Store::open(repo_root).context("open store")?;
    let existing = store.file_mtimes().context("read mtimes")?;

    let candidates = collect_candidates(repo_root)?;

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

fn collect_candidates(repo_root: &Path) -> Result<Vec<Candidate>> {
    let walker = WalkBuilder::new(repo_root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| {
            let n = e.file_name();
            n != ".git" && n != ".repoctx"
        })
        .build();

    let mut out = Vec::new();
    for dent in walker.flatten() {
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let abs = dent.path();
        let Some(language) = Language::from_path(abs) else {
            continue;
        };
        let rel = match abs.strip_prefix(repo_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_db = repoctx_store::to_db_path(rel);
        let meta = match dent.metadata() {
            Ok(m) => m,
            Err(e) => {
                debug!(path = %abs.display(), error = %e, "stat failed");
                continue;
            }
        };
        let size = meta.len();
        if size > MAX_FILE_BYTES {
            warn!(path = %abs.display(), size, "skipping file > 2 MiB");
            continue;
        }
        // Skim UTF-8 with a streaming read; defer real read to parser.
        if !is_utf8(abs) {
            warn!(path = %abs.display(), "skipping non-UTF-8 file");
            continue;
        }
        let mtime_ns = mtime_to_ns(&meta);
        out.push(Candidate {
            abs: abs.to_path_buf(),
            rel: rel_db,
            language,
            mtime_ns,
            size: size as i64,
        });
    }
    Ok(out)
}

fn mtime_to_ns(meta: &std::fs::Metadata) -> i64 {
    let m = meta.modified().unwrap_or(UNIX_EPOCH);
    match m.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos().min(i64::MAX as u128) as i64,
        Err(_) => 0,
    }
}

fn is_utf8(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => std::str::from_utf8(&bytes).is_ok(),
        Err(_) => false,
    }
}

