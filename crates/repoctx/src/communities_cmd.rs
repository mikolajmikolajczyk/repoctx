//! `repoctx communities` — graph clustering + god nodes (issue #14, the
//! "orientation layer").
//!
//! Runs single-level Louvain modularity optimization over the **resolved**
//! call graph (unambiguous, callable callees only — `located_edges` →
//! [`resolved_graph`], per-definition nodes) to
//! group symbols into subsystems, labels each cluster by its highest-degree
//! member, and surfaces god nodes (highest-degree symbols overall). Pure
//! topology — no embeddings, no LLM. Clustering over ambiguous fan-out would
//! produce garbage, so the input is resolved-only by construction (#14
//! guardrail).
//!
//! Louvain is hand-rolled (no crate in the tree); plain adjacency, not
//! petgraph — the modularity math wants degree bookkeeping, not a node store.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use repoctx_store::{LocatedEdge, Store};
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

const MAX_COMMUNITIES: usize = 30;
const MAX_MEMBERS: usize = 15;
const MAX_GOD_NODES: usize = 15;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Community {
    /// Highest-degree member — the cluster's representative symbol.
    pub label: String,
    pub size: usize,
    /// Members (capped), highest-degree first.
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GodNode {
    pub name: String,
    pub degree: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunitiesReport {
    pub nodes: usize,
    pub edges: usize,
    pub count: usize,
    pub communities: Vec<Community>,
    pub god_nodes: Vec<GodNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for CommunitiesReport {
    fn human(&self) -> String {
        let mut s = format!(
            "communities: {} clusters over {} symbols, {} resolved edges\n",
            self.count, self.nodes, self.edges
        );
        if !self.god_nodes.is_empty() {
            s.push_str("\n## god nodes (highest degree)\n");
            for g in &self.god_nodes {
                s.push_str(&format!("  {:<28} {} edges\n", g.name, g.degree));
            }
        }
        s.push_str("\n## clusters (by size)\n");
        for c in &self.communities {
            s.push_str(&format!(
                "  [{}] {} members — {}\n",
                c.size,
                c.label,
                c.members.join(", ")
            ));
        }
        if let Some(a) = &self.advisory {
            s.push_str("\nadvisory: ");
            s.push_str(a);
        }
        s.trim_end().to_string()
    }
}

pub fn run(repo_root: &Path, render: Render, gain_opts: GainOpts) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;
    let located = store.located_edges()?;

    let min_size = crate::config::Config::load(&store)?
        .analysis
        .subsystem_min_size;

    let graph = resolved_graph(&located).graph;
    let comm = graph.louvain();

    let (communities, total) =
        build_communities(&graph, &comm, min_size, MAX_COMMUNITIES, MAX_MEMBERS);
    let god_nodes = top_god_nodes(&graph, MAX_GOD_NODES);

    let advisory = advisory(graph.n, min_size);
    let report = CommunitiesReport {
        nodes: graph.n,
        edges: graph.edges().len(),
        count: total,
        communities,
        god_nodes,
        advisory,
    };
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "communities",
        None,
        &[],
    )
}

fn advisory(nodes: usize, min_size: usize) -> Option<String> {
    if nodes == 0 {
        return Some(
            "no resolved call edges — call graph empty or this repo's languages lack \
             call-graph coverage (core 8 only)"
                .to_string(),
        );
    }
    Some(format!(
        "subsystems = Louvain clusters with >= {min_size} members \
         (analysis.subsystem_min_size); count shared with `report`/`export`. Nodes \
         per-definition (same-named defs stay distinct, qualified name@file:line); \
         labels = highest-degree member. Receiver-aware, resolved edges only \
         (ADR-0010). Topology-only."
    ))
}

/// Group nodes into subsystems: clusters with `>= min_size` members, ranked by
/// size. Returns `(displayed, total)` where `displayed` is capped to
/// `max_communities` for listing and `total` is the full count of qualifying
/// subsystems — the shared "what is a subsystem" definition, so
/// `communities`/`report`/`export` all report the same number (issue #9 viz
/// reconciliation). `min_size` comes from `analysis.subsystem_min_size`.
pub(crate) fn build_communities(
    graph: &Graph,
    comm: &[usize],
    min_size: usize,
    max_communities: usize,
    max_members: usize,
) -> (Vec<Community>, usize) {
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node, &cid) in comm.iter().enumerate() {
        groups.entry(cid).or_default().push(node);
    }
    let mut communities: Vec<Community> = groups
        .into_values()
        .filter(|members| members.len() >= min_size)
        .map(|mut members| {
            // highest-degree first; label = top member.
            members.sort_by_key(|&n| std::cmp::Reverse(graph.degree(n)));
            let label = graph.name(members[0]).to_string();
            let size = members.len();
            let names = members
                .iter()
                .take(max_members)
                .map(|&n| graph.name(n).to_string())
                .collect();
            Community {
                label,
                size,
                members: names,
            }
        })
        .collect();
    communities.sort_by(|a, b| b.size.cmp(&a.size).then(a.label.cmp(&b.label)));
    let total = communities.len();
    communities.truncate(max_communities);
    (communities, total)
}

/// Top-degree nodes overall — the cross-cutting hubs. Shared by `report` (#15).
pub(crate) fn top_god_nodes(graph: &Graph, max: usize) -> Vec<GodNode> {
    let mut god_nodes: Vec<GodNode> = (0..graph.n)
        .map(|n| GodNode {
            name: graph.name(n).to_string(),
            degree: graph.degree(n),
        })
        .collect();
    god_nodes.sort_by(|a, b| b.degree.cmp(&a.degree).then(a.name.cmp(&b.name)));
    god_nodes.truncate(max);
    god_nodes
}

// ── Node-identity-correct graph construction ───────────────────────────────

/// A node's unique identity key: `(name, file, line)`. Two definitions sharing
/// a name (e.g. `set` in `event-bus.ts` and `storage.ts`) get distinct keys, so
/// they stay distinct graph nodes instead of collapsing into one fake super-hub
/// whose degree is the sum of unrelated definitions.
pub(crate) fn node_key(name: &str, file: &str, line: u32) -> String {
    format!("{name}\u{1}{file}\u{1}{line}")
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Resolved-only graph keyed by definition location, plus the per-node identity
/// keys (so consumers like `export` can map their own nodes to communities).
/// Only `callee_defs == 1` (unambiguous) edges feed degree/clustering —
/// extends the ADR-0010 resolved-only rule to god-node degree.
pub(crate) struct Resolved {
    pub graph: Graph,
    pub keys: Vec<String>,
}

/// Build the resolved call graph from located edges, with display labels that
/// stay bare (`getDB`) for unique names and qualify (`set@event-bus.ts:23`)
/// only when a name has multiple definitions.
pub(crate) fn resolved_graph(located: &[LocatedEdge]) -> Resolved {
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut keys: Vec<String> = Vec::new();
    let mut meta: Vec<(String, String, u32)> = Vec::new(); // (name, file, line)
    let mut edges: Vec<(usize, usize)> = Vec::new();

    let mut intern = |name: &str, file: &str, line: u32| -> usize {
        let key = node_key(name, file, line);
        if let Some(&i) = idx.get(&key) {
            return i;
        }
        let i = keys.len();
        idx.insert(key.clone(), i);
        keys.push(key);
        meta.push((name.to_string(), file.to_string(), line));
        i
    };

    for e in located.iter().filter(|e| e.callee_defs == 1) {
        let (cf, cl) = match (&e.callee_file, e.callee_line) {
            (Some(f), Some(l)) => (f.as_str(), l),
            _ => continue,
        };
        let a = intern(&e.caller_name, &e.caller_file, e.caller_line);
        let b = intern(&e.callee_name, cf, cl);
        edges.push((a, b));
    }

    // Qualify labels only for names with >1 definition.
    let mut name_count: HashMap<&str, usize> = HashMap::new();
    for (name, _, _) in &meta {
        *name_count.entry(name.as_str()).or_insert(0) += 1;
    }
    let labels: Vec<String> = meta
        .iter()
        .map(|(name, file, line)| {
            if name_count.get(name.as_str()).copied().unwrap_or(0) > 1 {
                // line stored 0-based (Tree-sitter native); display 1-based.
                format!("{name}@{}:{}", basename(file), line + 1)
            } else {
                name.clone()
            }
        })
        .collect();

    Resolved {
        graph: Graph::build(labels, &edges),
        keys,
    }
}

// ── Graph + Louvain ───────────────────────────────────────────────────────

/// Undirected weighted graph over symbol names, built from `(caller, callee)`
/// pairs. Parallel edges accumulate weight.
pub(crate) struct Graph {
    pub(crate) n: usize,
    names: Vec<String>,
    /// adjacency: node -> [(neighbor, weight)].
    adj: Vec<Vec<(usize, f64)>>,
    /// weighted degree per node.
    k: Vec<f64>,
    /// 2m = sum of all weighted degrees.
    m2: f64,
}

impl Graph {
    /// Build from pre-interned node labels + index edges. Parallel edges
    /// accumulate weight; self-loops dropped.
    pub(crate) fn build(names: Vec<String>, edges: &[(usize, usize)]) -> Self {
        let n = names.len();
        let mut wmap: HashMap<(usize, usize), f64> = HashMap::new();
        for &(a, b) in edges {
            if a == b {
                continue;
            }
            let key = if a < b { (a, b) } else { (b, a) };
            *wmap.entry(key).or_insert(0.0) += 1.0;
        }
        // Build adjacency in a DETERMINISTIC order: HashMap iteration is
        // randomized per process, and Louvain's local-moving result depends on
        // neighbor order, so iterating `wmap` directly made the partition (and
        // thus the subsystem count) differ between `communities`/`report`/
        // `export` runs. Sort the edges first so every invocation is identical.
        let mut keys: Vec<(usize, usize)> = wmap.keys().copied().collect();
        keys.sort_unstable();
        let mut adj = vec![Vec::new(); n];
        let mut k = vec![0.0; n];
        let mut m2 = 0.0;
        for (a, b) in keys {
            let w = wmap[&(a, b)];
            adj[a].push((b, w));
            adj[b].push((a, w));
            k[a] += w;
            k[b] += w;
            m2 += 2.0 * w;
        }
        Graph {
            n,
            names,
            adj,
            k,
            m2,
        }
    }

    /// Name-keyed build — for tests only. Production paths use
    /// [`resolved_graph`] so distinct same-named definitions stay distinct
    /// nodes (node-identity fix).
    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut idx: HashMap<&str, usize> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (a, b) in pairs {
            let ia = *idx.entry(a.as_str()).or_insert_with(|| {
                names.push(a.clone());
                names.len() - 1
            });
            let ib = *idx.entry(b.as_str()).or_insert_with(|| {
                names.push(b.clone());
                names.len() - 1
            });
            edges.push((ia, ib));
        }
        Graph::build(names, &edges)
    }

    pub(crate) fn name(&self, n: usize) -> &str {
        &self.names[n]
    }

    pub(crate) fn degree(&self, n: usize) -> usize {
        self.adj[n].len()
    }

    /// Unique undirected edges as `(a, b)` with `a < b`. For cross-cluster
    /// bridge detection in `report` (#15).
    pub(crate) fn edges(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for a in 0..self.n {
            for &(b, _) in &self.adj[a] {
                if a < b {
                    out.push((a, b));
                }
            }
        }
        out
    }

    /// Single-level Louvain local-moving phase. Returns a community id per
    /// node, relabeled contiguous. Good enough for orientation; full multilevel
    /// aggregation is overkill here.
    pub(crate) fn louvain(&self) -> Vec<usize> {
        let mut comm: Vec<usize> = (0..self.n).collect();
        let mut sigma_tot: Vec<f64> = self.k.clone();
        if self.m2 == 0.0 {
            return comm;
        }
        let mut improved = true;
        let mut passes = 0;
        while improved && passes < 20 {
            improved = false;
            passes += 1;
            for i in 0..self.n {
                let ci = comm[i];
                sigma_tot[ci] -= self.k[i];
                // sum of weights from i into each candidate community.
                let mut w_to: HashMap<usize, f64> = HashMap::new();
                for &(j, w) in &self.adj[i] {
                    *w_to.entry(comm[j]).or_insert(0.0) += w;
                }
                let mut best = ci;
                let mut best_gain =
                    w_to.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * self.k[i] / self.m2;
                // Iterate candidate communities in sorted order with a strict
                // `>` so ties resolve to the lowest community id — deterministic
                // regardless of HashMap iteration order (see `build`).
                let mut cands: Vec<(usize, f64)> = w_to.iter().map(|(&c, &w)| (c, w)).collect();
                cands.sort_unstable_by_key(|&(c, _)| c);
                for (c, w) in cands {
                    let gain = w - sigma_tot[c] * self.k[i] / self.m2;
                    if gain > best_gain {
                        best_gain = gain;
                        best = c;
                    }
                }
                sigma_tot[best] += self.k[i];
                if best != ci {
                    comm[i] = best;
                    improved = true;
                }
            }
        }
        relabel(&comm)
    }
}

/// Relabel community ids to a contiguous `0..k` range.
fn relabel(comm: &[usize]) -> Vec<usize> {
    let mut map: HashMap<usize, usize> = HashMap::new();
    comm.iter()
        .map(|&c| {
            let next = map.len();
            *map.entry(c).or_insert(next)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(cn: &str, cf: &str, cl: u32, kn: &str, kf: &str, kl: u32) -> LocatedEdge {
        LocatedEdge {
            caller_name: cn.into(),
            caller_file: cf.into(),
            caller_line: cl,
            callee_name: kn.into(),
            callee_defs: 1,
            callee_file: Some(kf.into()),
            callee_line: Some(kl),
        }
    }

    #[test]
    fn louvain_is_deterministic_across_builds() {
        // Two independent builds of the same graph must yield the SAME partition
        // — guards against HashMap-iteration nondeterminism that made
        // communities/report/export disagree on the subsystem count.
        let located: Vec<LocatedEdge> = (0..30)
            .map(|i| {
                let g = i / 5; // 6 clusters of 5
                edge(
                    &format!("f{i}"),
                    &format!("m{g}.ts"),
                    i,
                    &format!("f{}", (i + 1) % 30),
                    &format!("m{}.ts", ((i + 1) % 30) / 5),
                    (i + 1) % 30,
                )
            })
            .collect();
        let a = resolved_graph(&located).graph.louvain();
        let b = resolved_graph(&located).graph.louvain();
        assert_eq!(a, b, "Louvain partition must be reproducible");
    }

    #[test]
    fn same_name_defs_stay_distinct_nodes() {
        // Two distinct definitions of `helper` (different files) must NOT
        // collapse into one node — the node-identity fix.
        let located = vec![
            edge("helper", "a.ts", 0, "foo", "f.ts", 0),
            edge("helper", "b.ts", 0, "bar", "g.ts", 0),
        ];
        let r = resolved_graph(&located);
        assert_eq!(r.graph.n, 4, "helper@a, helper@b, foo, bar are distinct");
        let labels: Vec<&str> = (0..r.graph.n).map(|i| r.graph.name(i)).collect();
        // colliding name qualified with basename:1-based-line; unique stays bare.
        assert!(labels.contains(&"helper@a.ts:1"));
        assert!(labels.contains(&"helper@b.ts:1"));
        assert!(labels.contains(&"foo"));
        assert!(labels.contains(&"bar"));
    }

    #[test]
    fn ambiguous_callees_excluded_from_resolved_graph() {
        let mut amb = edge("caller", "a.ts", 0, "set", "x.ts", 0);
        amb.callee_defs = 3;
        amb.callee_file = None;
        amb.callee_line = None;
        let r = resolved_graph(&[amb]);
        assert_eq!(r.graph.n, 0, "ambiguous-only input yields an empty graph");
    }

    #[test]
    fn two_clusters_separate() {
        // Two triangles joined by a single bridge edge -> two communities.
        let pairs = vec![
            ("a1", "a2"),
            ("a2", "a3"),
            ("a3", "a1"),
            ("b1", "b2"),
            ("b2", "b3"),
            ("b3", "b1"),
            ("a1", "b1"), // bridge
        ];
        let pairs: Vec<(String, String)> = pairs
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let g = Graph::from_pairs(&pairs);
        assert_eq!(g.n, 6);
        let comm = g.louvain();
        // a-cluster shares a community; b-cluster shares one; the two differ.
        let cid = |name: &str| comm[g.names.iter().position(|x| x == name).unwrap()];
        assert_eq!(cid("a1"), cid("a2"));
        assert_eq!(cid("a2"), cid("a3"));
        assert_eq!(cid("b1"), cid("b2"));
        assert_ne!(
            cid("a2"),
            cid("b2"),
            "the two triangles are distinct clusters"
        );
    }

    #[test]
    fn empty_graph() {
        let g = Graph::from_pairs(&[]);
        assert_eq!(g.n, 0);
        assert!(g.louvain().is_empty());
    }
}
