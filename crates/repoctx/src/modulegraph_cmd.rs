//! `repoctx import-cycles` / `modules` — graph algorithms over the import
//! graph (issue #4, ADR-0011). First petgraph adopter — see
//! `wiki/decisions/2026-06-16-petgraph-for-graph-algos.md`: petgraph is an
//! **ephemeral** compute structure built per-command from a store query, run,
//! and dropped. SQLite stays the source of truth.
//!
//! Import edges are stored file → raw specifier. To get a file→file module
//! graph the shared [`ImportResolver`] resolves relative specifiers (`./x`)
//! AND **tsconfig path aliases** (`@adapters/*` → `src/adapters/*`, issue #8)
//! against the indexed file set. Bare/package specifiers (`react`) and
//! non-TS module syntax (Rust/Python/Go) stay external — counted, advised.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use repoctx_store::Store;
use serde::Serialize;

use crate::analysis_cmd::CycleReport;
use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;
use crate::resolver::ImportResolver;

const MAX_EDGES_OUT: usize = 500;

/// Resolved file→file import edges + how many edges were external/unresolved.
struct ResolvedGraph {
    edges: Vec<(String, String)>,
    external: usize,
}

fn resolve_graph(repo_root: &Path, store: &Store) -> Result<ResolvedGraph> {
    let files = store.all_file_paths()?;
    let raw = store.all_import_edges()?;
    let resolver = ImportResolver::load(repo_root, files);
    let mut edges = Vec::new();
    let mut external = 0usize;
    for (importer, spec) in raw {
        match resolver.resolve(&importer, &spec) {
            Some(target) => edges.push((importer, target)),
            None => external += 1,
        }
    }
    edges.sort();
    edges.dedup();
    Ok(ResolvedGraph { edges, external })
}

/// Build an ephemeral petgraph DiGraph from resolved edges (node weight = file
/// path), plus the path→index map.
fn build(edges: &[(String, String)]) -> (DiGraph<String, ()>, HashMap<String, NodeIndex>) {
    let mut g = DiGraph::<String, ()>::new();
    let mut idx: HashMap<String, NodeIndex> = HashMap::new();
    for (a, b) in edges {
        let na = *idx
            .entry(a.clone())
            .or_insert_with(|| g.add_node(a.clone()));
        let nb = *idx
            .entry(b.clone())
            .or_insert_with(|| g.add_node(b.clone()));
        g.add_edge(na, nb, ());
    }
    (g, idx)
}

// ── import-cycles ─────────────────────────────────────────────────────────

/// `repoctx import-cycles [--limit N]` — circular imports via petgraph SCC.
pub fn run_import_cycles(
    repo_root: &Path,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;
    let rg = resolve_graph(repo_root, &store)?;
    let (g, _) = build(&rg.edges);

    // SCCs with >1 member are import cycles (self-loops were filtered out).
    let mut cycles: Vec<Vec<String>> = tarjan_scc(&g)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut names: Vec<String> = scc.iter().map(|n| g[*n].clone()).collect();
            names.sort();
            names
        })
        .collect();
    cycles.sort();
    let cap = if limit > 0 { limit } else { cycles.len() };
    let truncated = cycles.len() > cap;
    cycles.truncate(cap);

    let advisory = import_cycle_advisory(&rg, cycles.is_empty(), truncated);
    let report = CycleReport {
        count: cycles.len(),
        cycles,
        advisory,
    };
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "import-cycles",
        None,
        &[],
    )
}

fn import_cycle_advisory(rg: &ResolvedGraph, empty: bool, truncated: bool) -> Option<String> {
    if truncated {
        return Some("output capped — pass a larger --limit".to_string());
    }
    if empty {
        return Some(format!(
            "no import cycles among {} resolved intra-repo edges ({} external edges \
             unresolved — bare/package imports + non-TS-alias languages)",
            rg.edges.len(),
            rg.external
        ));
    }
    Some(
        "members of each strongly-connected group import each other (directly or \
         transitively). Relative + tsconfig-alias resolution; bare/package edges excluded."
            .to_string(),
    )
}

// ── modules (dependency map) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ModuleMap {
    pub files: usize,
    pub edges: usize,
    pub external_edges: usize,
    pub cyclic: bool,
    /// Dependency-first order (a file appears after everything it imports).
    /// Empty when the graph is cyclic (no valid order).
    pub order: Vec<String>,
    /// Resolved file→file edges (capped).
    pub dependencies: Vec<Edge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

impl HumanRender for ModuleMap {
    fn human(&self) -> String {
        let mut s = format!(
            "module graph: {} files, {} intra-repo edges ({} external/alias), {}\n",
            self.files,
            self.edges,
            self.external_edges,
            if self.cyclic { "CYCLIC" } else { "acyclic" }
        );
        if self.cyclic {
            s.push_str("  (cyclic — no build order; run `repoctx import-cycles`)\n");
        }
        for e in self.dependencies.iter().take(40) {
            s.push_str(&format!("  {} → {}\n", e.from, e.to));
        }
        if self.dependencies.len() > 40 {
            s.push_str(&format!(
                "  … +{} more edges\n",
                self.dependencies.len() - 40
            ));
        }
        if let Some(a) = &self.advisory {
            s.push_str("\nadvisory: ");
            s.push_str(a);
        }
        s.trim_end().to_string()
    }
}

/// `repoctx modules` — the resolved import topology + a dependency-first order.
pub fn run_modules(repo_root: &Path, render: Render, gain_opts: GainOpts) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;
    let rg = resolve_graph(repo_root, &store)?;
    let (g, _) = build(&rg.edges);

    // toposort gives importer-before-imported; reverse for dependency-first.
    let (cyclic, order) = match toposort(&g, None) {
        Ok(mut topo) => {
            topo.reverse();
            (false, topo.iter().map(|n| g[*n].clone()).collect())
        }
        Err(_) => (true, Vec::new()),
    };

    let dependencies: Vec<Edge> = rg
        .edges
        .iter()
        .take(MAX_EDGES_OUT)
        .map(|(a, b)| Edge {
            from: a.clone(),
            to: b.clone(),
        })
        .collect();
    let advisory = modules_advisory(&rg, cyclic, dependencies.len());
    let report = ModuleMap {
        files: g.node_count(),
        edges: rg.edges.len(),
        external_edges: rg.external,
        cyclic,
        order,
        dependencies,
        advisory,
    };
    crate::gain::emit_and_record(&report, render, &mut store, gain_opts, "modules", None, &[])
}

fn modules_advisory(rg: &ResolvedGraph, cyclic: bool, shown: usize) -> Option<String> {
    if rg.edges.is_empty() {
        return Some(format!(
            "no resolved intra-repo import edges ({} external edges) — relative + \
             tsconfig-alias resolution; bare/package specifiers + non-TS languages \
             (Rust/Python/Go module resolution) stay external",
            rg.external
        ));
    }
    if cyclic {
        return Some("graph is cyclic — `order` omitted; see `repoctx import-cycles`".to_string());
    }
    if shown < rg.edges.len() {
        return Some(format!("edge list capped at {shown}; full counts above"));
    }
    None
}
