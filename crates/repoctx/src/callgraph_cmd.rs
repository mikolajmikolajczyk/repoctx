//! `repoctx callers <name>` / `repoctx callees <name>` — direct call-graph
//! edges (epic af42572 / ADR-0010). Name-based, accuracy class of
//! `definition`: ambiguous/unresolved edges are surfaced via the advisory.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use repoctx_backend::{CallEdge, CodeIntelBackend, TreeSitterBackend};
use repoctx_store::Store;
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{List, Render};
use crate::read_cmd;

/// Safety cap on transitive traversal so a hot symbol can't explode output.
const MAX_GRAPH_EDGES: usize = 2000;

/// Which direction to walk from `name`.
#[derive(Debug, Clone, Copy)]
pub enum Edges {
    /// Who calls `name`.
    Callers,
    /// What `name` calls.
    Callees,
}

impl Edges {
    fn command(self) -> &'static str {
        match self {
            Edges::Callers => "callers",
            Edges::Callees => "callees",
        }
    }
}

pub fn run(
    repo_root: &Path,
    name: String,
    edges: Edges,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);

    let mut hits = match edges {
        Edges::Callers => backend.callers(&name)?,
        Edges::Callees => backend.callees(&name)?,
    };
    if limit > 0 {
        hits.truncate(limit);
    }

    let advisory = advisory(&hits, &name);
    let candidate_paths = candidate_paths(&hits);
    let list = List::new(hits).with_advisory(advisory);

    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        edges.command(),
        Some(name.as_str()),
        &candidate_paths,
    )
}

/// Advisory for the name-based accuracy contract: flag empties (so a zero
/// result doesn't read as "definitely uncalled"), ambiguity, and unresolved
/// (external/dynamic) callees, pointing at `rg` as the fallback.
fn advisory(edges: &[CallEdge], name: &str) -> Option<String> {
    if edges.is_empty() {
        return Some(format!(
            "no call edges for `{name}` — it may be uncalled, defined in a \
             language without call-graph coverage, or invoked dynamically; \
             cross-check with `rg {name}`"
        ));
    }
    let ambiguous = edges.iter().filter(|e| e.ambiguous).count();
    let unresolved = edges.iter().filter(|e| e.callee.is_none()).count();
    if ambiguous > 0 || unresolved > 0 {
        return Some(format!(
            "name-based call graph (accuracy class of `definition`): \
             {ambiguous} ambiguous, {unresolved} unresolved (external/dynamic) \
             edge(s) — no receiver-type disambiguation; verify with `rg`"
        ));
    }
    None
}

/// Files an agent would otherwise have grepped to answer this: every caller
/// and resolved-callee file in the result.
fn candidate_paths(edges: &[CallEdge]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in edges {
        out.push(e.caller.location.path.clone());
        if let Some(c) = &e.callee {
            out.push(c.location.path.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

// ── Transitive traversal: `repoctx callgraph` (issue 520f710) ──────────

/// Traversal direction from the seed symbol.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    /// Follow callees (what the symbol calls), transitively.
    Down,
    /// Follow callers (who calls the symbol), transitively.
    Up,
    /// Both directions.
    Both,
}

impl Direction {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "down" => Ok(Direction::Down),
            "up" => Ok(Direction::Up),
            "both" => Ok(Direction::Both),
            other => bail!("invalid --direction `{other}` (expected up|down|both)"),
        }
    }
    fn down(self) -> bool {
        matches!(self, Direction::Down | Direction::Both)
    }
    fn up(self) -> bool {
        matches!(self, Direction::Up | Direction::Both)
    }
}

/// One edge in a transitive walk: a [`CallEdge`] tagged with the BFS depth
/// (1 = direct) and the direction it was reached by.
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub depth: u32,
    pub direction: &'static str,
    #[serde(flatten)]
    pub edge: CallEdge,
}

/// `repoctx callgraph <name> --depth N --direction up|down|both`.
/// Breadth-first over call edges, cycle-safe (visited set on symbol names),
/// bounded by `depth` and [`MAX_GRAPH_EDGES`].
pub fn run_graph(
    repo_root: &Path,
    name: String,
    depth: u32,
    direction: Direction,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);

    let mut visited: HashSet<String> = HashSet::from([name.clone()]);
    let mut frontier: Vec<String> = vec![name.clone()];
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut truncated = false;

    'outer: for d in 1..=depth {
        let mut next: Vec<String> = Vec::new();
        for sym in &frontier {
            if direction.down() {
                for e in backend.callees(sym)? {
                    let nxt = e.callee_name.clone();
                    edges.push(GraphEdge {
                        depth: d,
                        direction: "down",
                        edge: e,
                    });
                    if visited.insert(nxt.clone()) {
                        next.push(nxt);
                    }
                    if edges.len() >= MAX_GRAPH_EDGES {
                        truncated = true;
                        break 'outer;
                    }
                }
            }
            if direction.up() {
                for e in backend.callers(sym)? {
                    let nxt = e.caller.name.clone();
                    edges.push(GraphEdge {
                        depth: d,
                        direction: "up",
                        edge: e,
                    });
                    if visited.insert(nxt.clone()) {
                        next.push(nxt);
                    }
                    if edges.len() >= MAX_GRAPH_EDGES {
                        truncated = true;
                        break 'outer;
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    let inner: Vec<CallEdge> = edges.iter().map(|g| g.edge.clone()).collect();
    let advisory = graph_advisory(&inner, &name, truncated);
    let candidate_paths = candidate_paths(&inner);
    let list = List::new(edges).with_advisory(advisory);

    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &list,
        render,
        &mut store,
        gain_opts,
        "callgraph",
        Some(name.as_str()),
        &candidate_paths,
    )
}

fn graph_advisory(edges: &[CallEdge], name: &str, truncated: bool) -> Option<String> {
    if truncated {
        return Some(format!(
            "call graph truncated at {MAX_GRAPH_EDGES} edges — narrow with \
             a smaller --depth or a single --direction"
        ));
    }
    advisory(edges, name)
}
