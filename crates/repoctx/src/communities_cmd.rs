//! `repoctx communities` — graph clustering + god nodes (issue #14, the
//! "orientation layer").
//!
//! Runs single-level Louvain modularity optimization over the **resolved**
//! call graph (unambiguous, callable callees only — `resolved_edge_pairs`) to
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
use repoctx_store::Store;
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
    let pairs = store.resolved_edge_pairs()?;

    let graph = Graph::from_pairs(&pairs);
    let comm = graph.louvain();

    let communities = build_communities(&graph, &comm, MAX_COMMUNITIES, MAX_MEMBERS);
    let god_nodes = top_god_nodes(&graph, MAX_GOD_NODES);

    let advisory = advisory(graph.n);
    let report = CommunitiesReport {
        nodes: graph.n,
        edges: pairs.len(),
        count: communities.len(),
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

fn advisory(nodes: usize) -> Option<String> {
    if nodes == 0 {
        return Some(
            "no resolved call edges — call graph empty or this repo's languages lack \
             call-graph coverage (core 8 only)"
                .to_string(),
        );
    }
    Some(
        "clusters from single-level Louvain over resolved (unambiguous) call edges; \
         labels = highest-degree member. Topology-only, name-based (ADR-0010)."
            .to_string(),
    )
}

/// Group nodes by community id and build the ranked, capped `Community` list.
/// Shared by `communities` and `report` (#15).
pub(crate) fn build_communities(
    graph: &Graph,
    comm: &[usize],
    max_communities: usize,
    max_members: usize,
) -> Vec<Community> {
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node, &cid) in comm.iter().enumerate() {
        groups.entry(cid).or_default().push(node);
    }
    let mut communities: Vec<Community> = groups
        .into_values()
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
    communities.truncate(max_communities);
    communities
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
    pub(crate) fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut idx: HashMap<String, usize> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        fn intern(s: &str, idx: &mut HashMap<String, usize>, names: &mut Vec<String>) -> usize {
            if let Some(&i) = idx.get(s) {
                return i;
            }
            let i = names.len();
            names.push(s.to_string());
            idx.insert(s.to_string(), i);
            i
        }
        // weight by parallel-edge count (a calls b twice -> stronger tie).
        let mut wmap: HashMap<(usize, usize), f64> = HashMap::new();
        for (a, b) in pairs {
            let ia = intern(a, &mut idx, &mut names);
            let ib = intern(b, &mut idx, &mut names);
            if ia == ib {
                continue;
            }
            let key = if ia < ib { (ia, ib) } else { (ib, ia) };
            *wmap.entry(key).or_insert(0.0) += 1.0;
        }
        let n = names.len();
        let mut adj = vec![Vec::new(); n];
        let mut k = vec![0.0; n];
        let mut m2 = 0.0;
        for (&(a, b), &w) in &wmap {
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
                for (&c, &w) in &w_to {
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
