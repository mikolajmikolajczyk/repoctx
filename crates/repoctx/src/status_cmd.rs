//! `repoctx status` — index health, counts, freshness.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::output::{HumanRender, Render};
use crate::read_cmd;
use crate::walk::collect_stat;

#[derive(Debug, Serialize)]
pub struct LanguageCount {
    pub language: String,
    pub files: u64,
}

#[derive(Debug, Serialize)]
pub struct Staleness {
    pub changed: usize,
    pub new: usize,
    pub deleted: usize,
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub files: u64,
    pub symbols: u64,
    pub per_language: Vec<LanguageCount>,
    pub db_size_bytes: u64,
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staleness: Option<Staleness>,
}

impl HumanRender for StatusReport {
    fn human(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("schema_version: {}\n", self.schema_version));
        s.push_str(&format!("files:          {}\n", self.files));
        s.push_str(&format!("symbols:        {}\n", self.symbols));
        s.push_str(&format!("db_size_bytes:  {}\n", self.db_size_bytes));
        if !self.per_language.is_empty() {
            s.push_str("per_language:\n");
            for l in &self.per_language {
                s.push_str(&format!("  {:<12} {}\n", l.language, l.files));
            }
        }
        match &self.staleness {
            Some(st) => s.push_str(&format!(
                "staleness:      changed={} new={} deleted={}",
                st.changed, st.new, st.deleted
            )),
            None => s.push_str("staleness:      (skipped, --fast)"),
        }
        s
    }
}

pub fn run(repo_root: &Path, fast: bool, render: Render, no_auto_index: bool) -> Result<()> {
    read_cmd::ensure_db(repo_root, no_auto_index)?;
    let store = Store::open(repo_root).context("open store")?;
    let counts = store.counts().context("counts")?;
    let schema_version = store.schema_version().context("schema_version")?;

    let db_path = repo_root.join(".repoctx/index.db");
    let db_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let staleness = if fast {
        None
    } else {
        Some(compute_staleness(repo_root, &store)?)
    };

    let per_language: Vec<_> = counts
        .per_language
        .into_iter()
        .map(|(language, files)| LanguageCount { language, files })
        .collect();

    let report = StatusReport {
        files: counts.files,
        symbols: counts.symbols,
        per_language,
        db_size_bytes,
        schema_version,
        staleness,
    };
    crate::output::emit(&report, render)
}

fn compute_staleness(repo_root: &Path, store: &Store) -> Result<Staleness> {
    let existing = store.file_mtimes().context("read mtimes")?;
    let candidates = collect_stat(repo_root)?;
    let mut on_disk: HashSet<String> = HashSet::with_capacity(candidates.len());
    let mut changed = 0usize;
    let mut new = 0usize;
    for c in &candidates {
        on_disk.insert(c.rel.clone());
        match existing.get(&c.rel) {
            Some(&(m, s)) if m == c.mtime_ns && s == c.size => {}
            Some(_) => changed += 1,
            None => new += 1,
        }
    }
    let deleted = existing.keys().filter(|k| !on_disk.contains(*k)).count();
    Ok(Staleness {
        changed,
        new,
        deleted,
    })
}
