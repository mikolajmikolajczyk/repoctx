//! `repoctx report` — deterministic architecture report (issue #15, the
//! "orientation layer", idea 2 of 4).
//!
//! Composes the resolved call graph into a one-page markdown summary —
//! **generated entirely from topology, no LLM, no network**: god nodes,
//! communities (#14), cross-cluster bridges, entry points, and templated
//! "suggested questions" derived from structure. repoctx's identity is cheap +
//! deterministic; a topology-generated report preserves that. An opt-in
//! `--llm` prose layer is deferred (issue #15, doc-provider abstraction).
//!
//! Human render *is* the report markdown; `--out <path>` writes it to a file
//! (e.g. `REPORT.md`). JSON/TOON emit the structured data.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::communities_cmd::{
    build_communities, resolved_graph, top_god_nodes, Community, GodNode, Graph,
};
use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

const MAX_COMMUNITIES: usize = 12;
const MAX_MEMBERS: usize = 12;
const MAX_GOD_NODES: usize = 10;
const MAX_BRIDGES: usize = 12;
const MAX_ENTRY_POINTS: usize = 15;
const MAX_QUESTIONS: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct Bridge {
    /// Higher-degree endpoint of the cross-cluster edge.
    pub from: String,
    pub to: String,
    /// Cluster label (highest-degree member) of each endpoint.
    pub from_cluster: String,
    pub to_cluster: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportDoc {
    pub nodes: usize,
    pub edges: usize,
    pub communities_count: usize,
    pub god_nodes: Vec<GodNode>,
    pub communities: Vec<Community>,
    pub bridges: Vec<Bridge>,
    pub entry_points: Vec<EntryItem>,
    pub questions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for ReportDoc {
    fn human(&self) -> String {
        let mut s = String::new();
        s.push_str("# Repo architecture report\n\n");
        s.push_str(&format!(
            "_Deterministic, topology-only (no LLM). {} symbols, {} resolved call edges, \
             {} subsystems._\n",
            self.nodes, self.edges, self.communities_count
        ));

        s.push_str("\n## God nodes\n\n");
        s.push_str("_Highest-degree symbols — the cross-cutting hubs._\n\n");
        if self.god_nodes.is_empty() {
            s.push_str("_none_\n");
        } else {
            for g in &self.god_nodes {
                s.push_str(&format!("- `{}` — {} connections\n", g.name, g.degree));
            }
        }

        s.push_str("\n## Subsystems\n\n");
        s.push_str(
            "_Louvain clusters over the resolved call graph; label = the cluster's \
             highest-degree member._\n\n",
        );
        if self.communities.is_empty() {
            s.push_str("_none_\n");
        } else {
            for c in &self.communities {
                s.push_str(&format!(
                    "- **{}** ({} symbols) — {}\n",
                    c.label,
                    c.size,
                    c.members.join(", ")
                ));
            }
        }

        s.push_str("\n## Cross-cluster bridges\n\n");
        s.push_str(
            "_Call edges whose endpoints sit in different subsystems — the coupling \
             worth scrutinizing._\n\n",
        );
        if self.bridges.is_empty() {
            s.push_str("_none_\n");
        } else {
            for b in &self.bridges {
                s.push_str(&format!(
                    "- `{}` ({}) → `{}` ({})\n",
                    b.from, b.from_cluster, b.to, b.to_cluster
                ));
            }
        }

        s.push_str("\n## Entry points\n\n");
        if self.entry_points.is_empty() {
            s.push_str("_none detected_\n");
        } else {
            for e in &self.entry_points {
                s.push_str(&format!(
                    "- `{}` ({}) — {}:{}\n",
                    e.name, e.kind, e.path, e.line
                ));
            }
        }

        if !self.questions.is_empty() {
            s.push_str("\n## Suggested questions\n\n");
            for q in &self.questions {
                s.push_str(&format!("- {}\n", q));
            }
        }

        if let Some(a) = &self.advisory {
            s.push_str(&format!("\n---\n\n_{}_\n", a));
        }
        s.trim_end().to_string()
    }
}

pub fn run(
    repo_root: &Path,
    render: Render,
    gain_opts: GainOpts,
    out: Option<PathBuf>,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let mut store = Store::open(repo_root).context("open store")?;
    let located = store.located_edges()?;
    let min_size = crate::config::Config::load(&store)?.analysis.subsystem_min_size;

    let graph = resolved_graph(&located).graph;
    let comm = graph.louvain();

    let god_nodes = top_god_nodes(&graph, MAX_GOD_NODES);
    let (communities, subsystem_count) =
        build_communities(&graph, &comm, min_size, MAX_COMMUNITIES, MAX_MEMBERS);

    // Per-community label = highest-degree member, so bridges read in cluster
    // terms rather than raw ids.
    let cluster_label = community_labels(&graph, &comm);
    let bridges = cross_cluster_bridges(&graph, &comm, &cluster_label, MAX_BRIDGES);

    let entry_points: Vec<EntryItem> = store
        .entry_points()?
        .into_iter()
        .take(MAX_ENTRY_POINTS)
        .map(|s| EntryItem {
            name: s.name,
            kind: s.kind,
            path: s.file_path,
            line: s.start_line,
        })
        .collect();

    let questions = suggested_questions(&god_nodes, &communities, &bridges);

    let report = ReportDoc {
        nodes: graph.n,
        edges: graph.edges().len(),
        communities_count: subsystem_count,
        god_nodes,
        communities,
        bridges,
        entry_points,
        questions,
        advisory: advisory(graph.n),
    };

    if let Some(path) = out {
        // --out always writes the markdown form (the REPORT.md artifact),
        // regardless of --json/--toon.
        std::fs::write(&path, format!("{}\n", report.human()))
            .with_context(|| format!("write {}", path.display()))?;
        println!("wrote {}", path.display());
        return Ok(());
    }

    crate::gain::emit_and_record(&report, render, &mut store, gain_opts, "report", None, &[])
}

/// Map each community id to its representative label (highest-degree member).
fn community_labels(graph: &Graph, comm: &[usize]) -> HashMap<usize, String> {
    let mut best: HashMap<usize, (usize, usize)> = HashMap::new(); // cid -> (node, degree)
    for (node, &cid) in comm.iter().enumerate() {
        let d = graph.degree(node);
        let e = best.entry(cid).or_insert((node, d));
        if d > e.1 {
            *e = (node, d);
        }
    }
    best.into_iter()
        .map(|(cid, (node, _))| (cid, graph.name(node).to_string()))
        .collect()
}

/// Edges whose endpoints sit in different communities, ranked by combined
/// endpoint degree (high-traffic bridges = the surprising couplings).
fn cross_cluster_bridges(
    graph: &Graph,
    comm: &[usize],
    labels: &HashMap<usize, String>,
    max: usize,
) -> Vec<Bridge> {
    let mut scored: Vec<(usize, usize, usize)> = graph
        .edges()
        .into_iter()
        .filter(|&(a, b)| comm[a] != comm[b])
        .map(|(a, b)| (a, b, graph.degree(a) + graph.degree(b)))
        .collect();
    // Highest combined degree first; deterministic tie-break by name.
    scored.sort_by(|x, y| {
        y.2.cmp(&x.2)
            .then_with(|| graph.name(x.0).cmp(graph.name(y.0)))
            .then_with(|| graph.name(x.1).cmp(graph.name(y.1)))
    });
    scored
        .into_iter()
        .take(max)
        .map(|(a, b, _)| {
            // Present higher-degree endpoint as the `from` (the hub side).
            let (from, to) = if graph.degree(a) >= graph.degree(b) {
                (a, b)
            } else {
                (b, a)
            };
            Bridge {
                from: graph.name(from).to_string(),
                to: graph.name(to).to_string(),
                from_cluster: labels.get(&comm[from]).cloned().unwrap_or_default(),
                to_cluster: labels.get(&comm[to]).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

/// Templated questions derived purely from structure — orientation prompts, not
/// claims. No LLM.
fn suggested_questions(
    god_nodes: &[GodNode],
    communities: &[Community],
    bridges: &[Bridge],
) -> Vec<String> {
    let mut qs = Vec::new();
    if let Some(g) = god_nodes.first() {
        qs.push(format!(
            "`{}` is the highest-degree symbol ({} connections) — what does it coordinate, \
             and is that concentration intended?",
            g.name, g.degree
        ));
    }
    if let Some(c) = communities.first() {
        qs.push(format!(
            "the `{}` subsystem is the largest ({} symbols) — what is its single \
             responsibility?",
            c.label, c.size
        ));
    }
    for b in bridges.iter().take(MAX_QUESTIONS.saturating_sub(qs.len())) {
        qs.push(format!(
            "`{}` (in `{}`) reaches into `{}` (in `{}`) — is this cross-subsystem coupling \
             deliberate?",
            b.from, b.from_cluster, b.to, b.to_cluster
        ));
    }
    qs.truncate(MAX_QUESTIONS);
    qs
}

fn advisory(nodes: usize) -> Option<String> {
    if nodes == 0 {
        return Some(
            "no resolved call edges — call graph empty or this repo's languages lack \
             call-graph coverage (core 8 only). Report is empty."
                .to_string(),
        );
    }
    Some(
        "Generated from call-graph topology (name-based, ADR-0010). Deterministic, no LLM. \
         God-node degree + clustering use resolved (unambiguous) edges only, over \
         per-definition nodes (same-named defs stay distinct); host/builtin method \
         names (get/set/push/…) are excluded so they don't fake hubs. Suggested \
         questions are structural prompts, not findings."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(raw: &[(&str, &str)]) -> Vec<(String, String)> {
        raw.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn bridge_detected_between_clusters() {
        // Two triangles + a single bridge edge a1->b1.
        let p = pairs(&[
            ("a1", "a2"),
            ("a2", "a3"),
            ("a3", "a1"),
            ("b1", "b2"),
            ("b2", "b3"),
            ("b3", "b1"),
            ("a1", "b1"),
        ]);
        let g = Graph::from_pairs(&p);
        let comm = g.louvain();
        let labels = community_labels(&g, &comm);
        let bridges = cross_cluster_bridges(&g, &comm, &labels, 12);
        assert_eq!(bridges.len(), 1, "exactly one cross-cluster edge");
        let names = [bridges[0].from.as_str(), bridges[0].to.as_str()];
        assert!(names.contains(&"a1") && names.contains(&"b1"));
        assert_ne!(
            bridges[0].from_cluster, bridges[0].to_cluster,
            "bridge endpoints in different clusters"
        );
    }

    #[test]
    fn no_bridges_in_single_cluster() {
        let p = pairs(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let g = Graph::from_pairs(&p);
        let comm = g.louvain();
        let labels = community_labels(&g, &comm);
        assert!(cross_cluster_bridges(&g, &comm, &labels, 12).is_empty());
    }

    #[test]
    fn questions_templated_from_structure() {
        let p = pairs(&[("hub", "a"), ("hub", "b"), ("hub", "c"), ("a", "b")]);
        let g = Graph::from_pairs(&p);
        let comm = g.louvain();
        let god = top_god_nodes(&g, 10);
        let (comms, _) = build_communities(&g, &comm, 2, 12, 12);
        let labels = community_labels(&g, &comm);
        let bridges = cross_cluster_bridges(&g, &comm, &labels, 12);
        let qs = suggested_questions(&god, &comms, &bridges);
        assert!(!qs.is_empty());
        assert!(qs[0].contains("hub"), "top god node drives first question");
    }
}
