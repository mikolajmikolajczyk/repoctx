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
const MAX_PUBLIC_MODULES: usize = 20;
const MAX_PUBLIC_SYMBOLS_PER_MODULE: usize = 12;

#[derive(Debug, Clone, Serialize)]
pub struct LanguageStat {
    pub lang: String,
    pub symbols: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStat {
    pub dir: String,
    pub files: u64,
    /// Code symbols (excludes markdown headings + config keys). Ranking metric.
    pub code_symbols: u64,
    /// Total symbols including doc/config (markdown `section`, config `key`).
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
pub struct PublicModule {
    pub dir: String,
    /// Total public symbols in this directory.
    pub count: u64,
    /// Exported symbol names (capped), `name:kind`.
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub files: u64,
    pub symbols: u64,
    /// Code symbols only (total minus markdown headings + config keys).
    pub code_symbols: u64,
    pub languages: Vec<LanguageStat>,
    pub modules: Vec<ModuleStat>,
    /// Exported (public-visibility) symbols per directory (issue #10). Empty for
    /// repos whose languages have no visibility extractor yet (all `unknown`).
    pub public_api: Vec<PublicModule>,
    pub entry_points: Vec<EntryPoint>,
    pub hotspots: Vec<Hotspot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for Overview {
    fn human(&self) -> String {
        let docs = self.symbols.saturating_sub(self.code_symbols);
        let mut s = format!(
            "# overview — {} files, {} code symbols ({} total, {} doc/config)\n",
            self.files, self.code_symbols, self.symbols, docs
        );

        s.push_str("\n## languages\n");
        for l in &self.languages {
            s.push_str(&format!("  {:<12} {}\n", l.lang, l.symbols));
        }

        s.push_str("\n## modules (by code symbols)\n");
        for m in &self.modules {
            s.push_str(&format!(
                "  {:<32} {} files, {} code symbols, {} B\n",
                m.dir, m.files, m.code_symbols, m.bytes
            ));
        }

        if !self.public_api.is_empty() {
            s.push_str("\n## public API (exported symbols by module)\n");
            for m in &self.public_api {
                s.push_str(&format!(
                    "  {:<32} {} exported: {}\n",
                    m.dir,
                    m.count,
                    m.symbols.join(", ")
                ));
            }
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
    // Rank by CODE symbols so doc/config dirs (wiki, .github) don't top the
    // list on heading/key counts (issue #9-D).
    let sizes = store.file_sizes()?;
    let sym_counts: HashMap<String, (u64, u64)> = store
        .symbol_counts_by_file()?
        .into_iter()
        .map(|(path, total, code)| (path, (total, code)))
        .collect();
    let mut code_total = 0u64;
    let mut by_dir: HashMap<String, ModuleStat> = HashMap::new();
    for (path, size) in &sizes {
        let dir = parent_dir(path).to_string();
        let e = by_dir.entry(dir.clone()).or_insert(ModuleStat {
            dir,
            files: 0,
            code_symbols: 0,
            symbols: 0,
            bytes: 0,
        });
        let (total, code) = sym_counts.get(path).copied().unwrap_or((0, 0));
        e.files += 1;
        e.bytes += *size;
        e.symbols += total;
        e.code_symbols += code;
        code_total += code;
    }
    let mut modules: Vec<ModuleStat> = by_dir.into_values().collect();
    modules.sort_by(|a, b| {
        b.code_symbols
            .cmp(&a.code_symbols)
            .then(b.symbols.cmp(&a.symbols))
            .then(a.dir.cmp(&b.dir))
    });
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

    // Public API surface (#10): exported symbols folded by directory.
    let mut pub_by_dir: HashMap<String, Vec<String>> = HashMap::new();
    for (path, name, kind) in store.public_symbols()? {
        pub_by_dir
            .entry(parent_dir(&path).to_string())
            .or_default()
            .push(format!("{name}:{kind}"));
    }
    let mut public_api: Vec<PublicModule> = pub_by_dir
        .into_iter()
        .map(|(dir, mut symbols)| {
            symbols.sort();
            let count = symbols.len() as u64;
            symbols.truncate(MAX_PUBLIC_SYMBOLS_PER_MODULE);
            PublicModule {
                dir,
                count,
                symbols,
            }
        })
        .collect();
    public_api.sort_by(|a, b| b.count.cmp(&a.count).then(a.dir.cmp(&b.dir)));
    public_api.truncate(MAX_PUBLIC_MODULES);

    let advisory = overview_advisory(hotspots.is_empty(), public_api.is_empty());
    let report = Overview {
        files: counts.files,
        symbols: counts.symbols,
        code_symbols: code_total,
        languages,
        modules,
        public_api,
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

fn overview_advisory(no_hotspots: bool, no_public_api: bool) -> Option<String> {
    let mut notes = vec![
        "modules ranked by code symbols; markdown headings + config keys counted \
         as doc/config, not code (#9-D)",
    ];
    if no_public_api {
        notes.push(
            "public API surface empty — visibility is extracted for Go/Rust/JS/TS \
             only; other languages are 'unknown' (#10)",
        );
    }
    if no_hotspots {
        notes.push(
            "no hotspots: call graph empty or this repo's languages lack call-graph \
             coverage (core 8)",
        );
    }
    Some(notes.join("; "))
}
