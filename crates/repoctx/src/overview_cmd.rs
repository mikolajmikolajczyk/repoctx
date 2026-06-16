//! `repoctx overview` — repo architecture in one call (issue #5).
//!
//! Synthesises what the index + call graph already know: totals, per-language
//! breakdown, per-directory module sizes, entry points, and hotspots (most-
//! called symbols). The "agent dropped into an unfamiliar repo" command —
//! replaces dozens of `ls`/`cat`/grep round-trips with one structural map.
//!
//! Public API surface (exported symbols per module) is intentionally absent:
//! it needs per-language `pub`/`export` extraction that isn't built yet (see
//! the import-graph epic #4 close + #8). Hotspots are name-based (ADR-0010).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

const MAX_MODULES: usize = 30;
const DEFAULT_HOTSPOTS: usize = 15;
const MAX_ENTRY_POINTS: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct LanguageStat {
    pub lang: String,
    pub symbols: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStat {
    pub dir: String,
    pub files: u64,
    pub symbols: u64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryPoint {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    pub name: String,
    pub callers: u64,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub files: u64,
    pub symbols: u64,
    pub languages: Vec<LanguageStat>,
    pub modules: Vec<ModuleStat>,
    pub entry_points: Vec<EntryPoint>,
    pub hotspots: Vec<Hotspot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for Overview {
    fn human(&self) -> String {
        let mut s = format!(
            "# overview — {} files, {} symbols\n",
            self.files, self.symbols
        );

        s.push_str("\n## languages\n");
        for l in &self.languages {
            s.push_str(&format!("  {:<12} {}\n", l.lang, l.symbols));
        }

        s.push_str("\n## modules (by symbols)\n");
        for m in &self.modules {
            s.push_str(&format!(
                "  {:<32} {} files, {} symbols, {} B\n",
                m.dir, m.files, m.symbols, m.bytes
            ));
        }

        if !self.entry_points.is_empty() {
            s.push_str("\n## entry points\n");
            for e in &self.entry_points {
                s.push_str(&format!(
                    "  {}:{}  {} {}\n",
                    e.path,
                    e.line + 1,
                    e.name,
                    e.kind
                ));
            }
        }

        if !self.hotspots.is_empty() {
            s.push_str("\n## hotspots (most called)\n");
            for h in &self.hotspots {
                s.push_str(&format!(
                    "  {:<28} {} callers  {}:{}\n",
                    h.name,
                    h.callers,
                    h.path,
                    h.line + 1
                ));
            }
        }

        if let Some(a) = &self.advisory {
            s.push_str("\nadvisory: ");
            s.push_str(a);
        }
        s.trim_end().to_string()
    }
}

/// Parent directory of a `/`-separated db path, or `.` for a root file.
fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => ".",
    }
}

pub fn run(repo_root: &Path, render: Render, gain_opts: GainOpts) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;

    let counts = store.counts().context("counts")?;
    let languages: Vec<LanguageStat> = counts
        .per_language
        .iter()
        .map(|(lang, n)| LanguageStat {
            lang: lang.clone(),
            symbols: *n,
        })
        .collect();

    // Module stats: fold files + per-file symbol counts by parent directory.
    let sizes = store.file_sizes()?;
    let sym_counts: HashMap<String, u64> = store.symbol_counts_by_file()?.into_iter().collect();
    let mut by_dir: HashMap<String, ModuleStat> = HashMap::new();
    for (path, size) in &sizes {
        let dir = parent_dir(path).to_string();
        let e = by_dir.entry(dir.clone()).or_insert(ModuleStat {
            dir,
            files: 0,
            symbols: 0,
            bytes: 0,
        });
        e.files += 1;
        e.bytes += *size;
        e.symbols += sym_counts.get(path).copied().unwrap_or(0);
    }
    let mut modules: Vec<ModuleStat> = by_dir.into_values().collect();
    modules.sort_by(|a, b| b.symbols.cmp(&a.symbols).then(a.dir.cmp(&b.dir)));
    modules.truncate(MAX_MODULES);

    let mut entry_points: Vec<EntryPoint> = store
        .entry_points()?
        .into_iter()
        .map(|s| EntryPoint {
            name: s.name,
            kind: s.kind,
            path: s.file_path,
            line: s.start_line,
        })
        .collect();
    entry_points.truncate(MAX_ENTRY_POINTS);

    let hotspots: Vec<Hotspot> = store
        .hotspots(DEFAULT_HOTSPOTS)?
        .into_iter()
        .map(|(name, callers, path, line)| Hotspot {
            name,
            callers,
            path,
            line,
        })
        .collect();

    let advisory = overview_advisory(hotspots.is_empty());
    let report = Overview {
        files: counts.files,
        symbols: counts.symbols,
        languages,
        modules,
        entry_points,
        hotspots,
        advisory,
    };
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "overview",
        None,
        &[],
    )
}

fn overview_advisory(no_hotspots: bool) -> Option<String> {
    let mut notes = vec![
        "public API surface (exported symbols per module) not included yet — needs \
         per-language export extraction (#8)",
    ];
    if no_hotspots {
        notes.push(
            "no hotspots: call graph empty or this repo's languages lack call-graph \
             coverage (core 8)",
        );
    }
    Some(notes.join("; "))
}
