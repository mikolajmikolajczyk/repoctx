//! `repoctx deadcode` / `impact` / `cycles` — Tier-1 analyses over the
//! existing call graph (issue #3). No new indexing; pure queries + graph
//! walks over the schema-v4 `calls` table.
//!
//! All three inherit the call graph's **name-based** accuracy class
//! (ADR-0010): edges are resolved by name, so dynamic dispatch, trait
//! objects, FFI, and out-of-scope (public-API) callers are invisible. Output
//! is advisory — a strong hint, not proof.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use repoctx_backend::{Location, Symbol, SymbolKind};
use repoctx_store::{Store, SymbolRecord};
use serde::Serialize;

use crate::callgraph_cmd::{self, Direction};
use crate::gain::GainOpts;
use crate::output::{HumanRender, List, Render};
use crate::read_cmd;

/// Names that look like entry points, so dead-code doesn't flag them. Kept
/// deliberately small + conservative; the advisory covers the rest.
const ENTRY_POINT_NAMES: &[&str] = &["main"];

fn to_symbol(r: SymbolRecord) -> Symbol {
    Symbol {
        name: r.name,
        kind: SymbolKind::from_str(&r.kind).unwrap_or(SymbolKind::Other),
        location: Location {
            path: r.file_path,
            start_line: r.start_line,
            start_column: r.start_column,
            end_line: r.end_line,
            end_column: r.end_column,
        },
    }
}

/// `repoctx deadcode [--lang L] [--limit N]` — function/method symbols with
/// zero incoming call edges (and not an entry point). Grep cannot do this.
pub fn run_deadcode(
    repo_root: &Path,
    lang: Option<String>,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;

    let mut rows = store.uncalled_symbols(lang.as_deref())?;
    rows.retain(|s| !ENTRY_POINT_NAMES.contains(&s.name.as_str()));
    if limit > 0 {
        rows.truncate(limit);
    }
    let candidate_paths: Vec<String> = {
        let mut p: Vec<String> = rows.iter().map(|s| s.file_path.clone()).collect();
        p.sort();
        p.dedup();
        p
    };
    let advisory = deadcode_advisory(rows.is_empty());
    let items: Vec<Symbol> = rows.into_iter().map(to_symbol).collect();
    let list = List::new(items).with_advisory(advisory);

    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "deadcode",
        None,
        &candidate_paths,
    )
}

fn deadcode_advisory(empty: bool) -> Option<String> {
    if empty {
        return Some(
            "no uncalled functions/methods found — or this language lacks \
             call-graph coverage (core 8 only)"
                .to_string(),
        );
    }
    Some(
        "name-based: these have no in-repo caller, but may be called \
         dynamically / via traits / FFI. Exported/public symbols (where the \
         language has visibility extraction — Go + JS/TS inline `export`), test files, \
         `.d.ts` decls, and minified/generated files are already excluded. \
         Treat as candidates — verify before deleting."
            .to_string(),
    )
}

/// `repoctx impact <name> [--depth N]` — blast radius: everything that
/// transitively *calls* `name` ("if I change X, what breaks"). A framed alias
/// for `callgraph <name> --direction up`.
pub fn run_impact(
    repo_root: &Path,
    name: String,
    depth: u32,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    callgraph_cmd::run_graph(repo_root, name, depth, Direction::Up, render, gain_opts)
}

// ── Cycles ──────────────────────────────────────────────────────────────

/// Guard: skip cycle detection on very large graphs rather than churn.
const MAX_CYCLE_EDGES: usize = 20_000;
/// Cap reported cycles.
const MAX_CYCLES: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct CycleReport {
    pub count: usize,
    /// Each cycle is a chain of symbol names; the first repeats at the end
    /// implicitly (A → B → C → A is `[A, B, C]`).
    pub cycles: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for CycleReport {
    fn human(&self) -> String {
        let mut s = if self.cycles.is_empty() {
            "no call cycles found".to_string()
        } else {
            let mut out = format!("{} call cycle(s):\n", self.count);
            for (i, c) in self.cycles.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                out.push_str(&format!(
                    "  {} → {}",
                    c.join(" → "),
                    c.first().cloned().unwrap_or_default()
                ));
            }
            out
        };
        if let Some(a) = &self.advisory {
            s.push_str("\n\nadvisory: ");
            s.push_str(a);
        }
        s
    }
}

/// `repoctx cycles [--limit N]` — detect cycles in the call graph (recursion
/// or mutual recursion). Name-based, in-repo edges only.
pub fn run_cycles(
    repo_root: &Path,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;

    let pairs = store.resolved_edge_pairs()?;
    let cap = if limit > 0 {
        limit.min(MAX_CYCLES)
    } else {
        MAX_CYCLES
    };

    let (mut cycles, mut truncated) = if pairs.len() > MAX_CYCLE_EDGES {
        (Vec::new(), true)
    } else {
        find_cycles(&pairs, cap)
    };
    let too_large = pairs.len() > MAX_CYCLE_EDGES;
    cycles.truncate(cap);
    if cycles.len() >= cap {
        truncated = true;
    }

    let advisory = cycles_advisory(too_large, truncated, cycles.is_empty());
    let report = CycleReport {
        count: cycles.len(),
        cycles,
        advisory,
    };
    crate::gain::emit_and_record(&report, render, &mut store, gain_opts, "cycles", None, &[])
}

fn cycles_advisory(too_large: bool, truncated: bool, empty: bool) -> Option<String> {
    if too_large {
        return Some(format!(
            "call graph too large ({MAX_CYCLE_EDGES}+ edges) — cycle detection skipped"
        ));
    }
    if truncated {
        return Some(format!("output capped at {MAX_CYCLES} cycles"));
    }
    if empty {
        return Some(
            "no cycles — or this language lacks call-graph coverage (core 8 only)".to_string(),
        );
    }
    Some("name-based: ambiguous names can fabricate or hide cycles; verify the chain".to_string())
}

/// Iterative DFS cycle finder over `(caller, callee)` pairs. Returns distinct
/// cycles (rotated to a canonical start to dedupe) and whether `max` capped it.
fn find_cycles(pairs: &[(String, String)], max: usize) -> (Vec<Vec<String>>, bool) {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in pairs {
        adj.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut color: HashMap<&str, u8> = HashMap::new(); // 0 white,1 gray,2 black
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    let nodes: Vec<&str> = adj.keys().copied().collect();
    for &start in &nodes {
        if color.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        let mut path: Vec<&str> = vec![start];
        color.insert(start, 1);
        while let Some(&(node, idx)) = stack.last() {
            let children = adj.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if idx < children.len() {
                stack.last_mut().unwrap().1 += 1;
                let nb = children[idx];
                match color.get(nb).copied().unwrap_or(0) {
                    0 => {
                        color.insert(nb, 1);
                        stack.push((nb, 0));
                        path.push(nb);
                    }
                    1 => {
                        if let Some(pos) = path.iter().position(|n| *n == nb) {
                            let cyc = canonical_cycle(&path[pos..]);
                            if seen.insert(cyc.clone()) {
                                cycles.push(cyc);
                                if cycles.len() >= max {
                                    return (cycles, true);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                color.insert(node, 2);
                stack.pop();
                path.pop();
            }
        }
    }
    (cycles, false)
}

/// Rotate a cycle so its lexicographically-smallest node is first, for
/// dedupe across discovery order / start node.
fn canonical_cycle(slice: &[&str]) -> Vec<String> {
    let min_pos = slice
        .iter()
        .enumerate()
        .min_by_key(|(_, n)| **n)
        .map(|(i, _)| i)
        .unwrap_or(0);
    slice[min_pos..]
        .iter()
        .chain(slice[..min_pos].iter())
        .map(|s| s.to_string())
        .collect()
}
