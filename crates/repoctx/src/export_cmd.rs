//! `repoctx export` — self-contained interactive graph HTML (issue #16, the
//! "orientation layer", idea 3 of 4).
//!
//! Emits ONE HTML file with the call graph embedded as JSON and a tiny
//! hand-rolled force-directed layout in vanilla JS — **no CDN, no build step,
//! no server**. Nodes are colored by community (#14), sized by degree;
//! edges are styled by `ambiguous` status (the differentiator — repoctx knows
//! which edges are uncertain, so the viz can *show* it). Filter by community,
//! search by symbol.
//!
//! `--out <path>` writes the file; otherwise the HTML goes to stdout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::communities_cmd::Graph;
use crate::read_cmd;

const TEMPLATE: &str = include_str!("export_template.html");

#[derive(Debug, Serialize)]
struct VizNode {
    name: String,
    /// Community id, or `-1` if the node has no resolved-edge membership.
    community: i64,
    degree: usize,
}

#[derive(Debug, Serialize)]
struct VizEdge {
    source: usize,
    target: usize,
    /// True when the callee name has >1 code definition (name-based ambiguity).
    ambiguous: bool,
}

#[derive(Debug, Serialize)]
struct VizData {
    nodes: Vec<VizNode>,
    edges: Vec<VizEdge>,
}

pub fn run(repo_root: &Path, out: Option<PathBuf>) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let raw = store.graph_edges()?;

    // Keep internal edges only (callee has >=1 code def); drop external/library
    // calls so the graph stays about this repo. defs==1 resolved, >1 ambiguous.
    let kept: Vec<(&str, &str, bool)> = raw
        .iter()
        .filter(|(_, _, defs)| *defs >= 1)
        .map(|(a, b, defs)| (a.as_str(), b.as_str(), *defs > 1))
        .collect();

    // Communities over the *resolved* subgraph (unambiguous edges), same basis
    // as `communities`/`report`.
    let resolved_pairs: Vec<(String, String)> = kept
        .iter()
        .filter(|(_, _, amb)| !amb)
        .map(|(a, b, _)| (a.to_string(), b.to_string()))
        .collect();
    let cgraph = Graph::from_pairs(&resolved_pairs);
    let comm = cgraph.louvain();
    let mut community_of: HashMap<String, i64> = HashMap::new();
    for (i, &cid) in comm.iter().enumerate() {
        community_of.insert(cgraph.name(i).to_string(), cid as i64);
    }

    // Node index over every endpoint of a kept edge.
    let mut idx: HashMap<&str, usize> = HashMap::new();
    let mut names: Vec<&str> = Vec::new();
    let mut degree: Vec<usize> = Vec::new();
    let mut edges: Vec<VizEdge> = Vec::with_capacity(kept.len());
    for (a, b, amb) in &kept {
        let ia = intern_node(a, &mut idx, &mut names, &mut degree);
        let ib = intern_node(b, &mut idx, &mut names, &mut degree);
        degree[ia] += 1;
        degree[ib] += 1;
        edges.push(VizEdge {
            source: ia,
            target: ib,
            ambiguous: *amb,
        });
    }

    let nodes: Vec<VizNode> = names
        .iter()
        .enumerate()
        .map(|(i, &n)| VizNode {
            name: n.to_string(),
            community: community_of.get(n).copied().unwrap_or(-1),
            degree: degree[i],
        })
        .collect();

    let data = VizData { nodes, edges };
    let json = serde_json::to_string(&data).context("serialize graph")?;
    let html = TEMPLATE.replace("/*__DATA__*/", &json).replace(
        "__SUBTITLE__",
        &format!(
            "{} symbols · {} edges · {} subsystems",
            data_node_count(&data),
            data.edges.len(),
            distinct_communities(&data),
        ),
    );

    match out {
        Some(path) => {
            std::fs::write(&path, &html).with_context(|| format!("write {}", path.display()))?;
            println!("wrote {} ({} bytes)", path.display(), html.len());
        }
        None => print!("{html}"),
    }
    Ok(())
}

fn data_node_count(d: &VizData) -> usize {
    d.nodes.len()
}

fn distinct_communities(d: &VizData) -> usize {
    let mut set: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for n in &d.nodes {
        if n.community >= 0 {
            set.insert(n.community);
        }
    }
    set.len()
}

/// Intern a node name into the parallel `idx`/`names`/`degree` vectors,
/// returning its index. New nodes start at degree 0.
fn intern_node<'a>(
    s: &'a str,
    idx: &mut HashMap<&'a str, usize>,
    names: &mut Vec<&'a str>,
    degree: &mut Vec<usize>,
) -> usize {
    if let Some(&i) = idx.get(s) {
        return i;
    }
    let i = names.len();
    idx.insert(s, i);
    names.push(s);
    degree.push(0);
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_communities_counts_nonnegative() {
        let d = VizData {
            nodes: vec![
                VizNode {
                    name: "a".into(),
                    community: 0,
                    degree: 1,
                },
                VizNode {
                    name: "b".into(),
                    community: 0,
                    degree: 1,
                },
                VizNode {
                    name: "c".into(),
                    community: -1,
                    degree: 0,
                },
            ],
            edges: vec![],
        };
        assert_eq!(distinct_communities(&d), 1);
        assert_eq!(data_node_count(&d), 3);
    }
}
