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

use crate::communities_cmd::{node_key, resolved_graph};
use crate::read_cmd;

const TEMPLATE: &str = include_str!("export_template.html");

#[derive(Debug, Serialize)]
struct VizNode {
    name: String,
    /// Community id, or `-1` if the node has no resolved-edge membership.
    community: i64,
    degree: usize,
    /// True when this node's community is a real subsystem
    /// (`>= analysis.subsystem_min_size` members). Non-major + unclustered
    /// nodes render grey so the eye reads the real subsystems as colored
    /// islands against a grey tail (the long Louvain tail + ambiguous layer).
    major: bool,
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

/// Honest count breakdown for the subtitle — neither hides the resolved
/// structure nor the ambiguity layer (the "ambiguous honesty" repoctx applies
/// everywhere).
struct Counts {
    subsystems: usize,
    resolved_nodes: usize,
    ambiguous_nodes: usize,
    resolved_edges: usize,
    ambiguous_edges: usize,
}

pub fn run(repo_root: &Path, out: Option<PathBuf>) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let located = store.located_edges()?;
    let min_size = crate::config::Config::load(&store)?.analysis.subsystem_min_size;

    // Communities + node identity over the resolved subgraph — same basis as
    // `communities`/`report`. Map each definition's identity key to its
    // community so viz nodes color consistently (node-identity fix).
    let resolved = resolved_graph(&located);
    let comm = resolved.graph.louvain();
    let mut community_of: HashMap<&str, i64> = HashMap::new();
    for (i, key) in resolved.keys.iter().enumerate() {
        community_of.insert(key.as_str(), comm[i] as i64);
    }
    // Major communities = subsystems (>= min_size members) — the SAME definition
    // `communities`/`report` use, so the subtitle count matches them exactly.
    let mut comm_size: HashMap<i64, usize> = HashMap::new();
    for &c in &comm {
        *comm_size.entry(c as i64).or_insert(0) += 1;
    }
    let major: std::collections::HashSet<i64> = comm_size
        .iter()
        .filter(|(_, &n)| n >= min_size)
        .map(|(&c, _)| c)
        .collect();

    // Build viz nodes keyed by definition location. A resolved callee uses its
    // unique def key; an ambiguous callee (N defs, location unknown) buckets to
    // one name node, drawn with dashed edges so the uncertainty is visible.
    let mut acc = NodeAcc::default();
    let mut edges: Vec<VizEdge> = Vec::with_capacity(located.len());
    for e in &located {
        let ckey = node_key(&e.caller_name, &e.caller_file, e.caller_line);
        let ia = acc.intern(ckey, &e.caller_name, Some(&e.caller_file), Some(e.caller_line));
        let (ib, ambiguous) = match (e.callee_defs, &e.callee_file, e.callee_line) {
            (1, Some(f), Some(l)) => {
                let key = node_key(&e.callee_name, f, l);
                (acc.intern(key, &e.callee_name, Some(f), Some(l)), false)
            }
            _ => {
                let key = format!("{}\u{1}?", e.callee_name);
                (acc.intern(key, &e.callee_name, None, None), true)
            }
        };
        acc.degree[ia] += 1;
        acc.degree[ib] += 1;
        edges.push(VizEdge {
            source: ia,
            target: ib,
            ambiguous,
        });
    }

    let nodes = acc.into_nodes(&community_of, &major);
    let counts = Counts {
        subsystems: major.len(),
        resolved_nodes: nodes.iter().filter(|n| n.community >= 0).count(),
        ambiguous_nodes: nodes.iter().filter(|n| n.community < 0).count(),
        resolved_edges: edges.iter().filter(|e| !e.ambiguous).count(),
        ambiguous_edges: edges.iter().filter(|e| e.ambiguous).count(),
    };
    let data = VizData { nodes, edges };
    let json = serde_json::to_string(&data).context("serialize graph")?;
    let subtitle = format!(
        "{} subsystems · {} symbols ({} resolved + {} ambiguous/builtin) · \
         {} edges ({} resolved + {} ambiguous)",
        counts.subsystems,
        counts.resolved_nodes + counts.ambiguous_nodes,
        counts.resolved_nodes,
        counts.ambiguous_nodes,
        counts.resolved_edges + counts.ambiguous_edges,
        counts.resolved_edges,
        counts.ambiguous_edges,
    );
    let html = TEMPLATE
        .replace("/*__DATA__*/", &json)
        .replace("__SUBTITLE__", &subtitle);

    match out {
        Some(path) => {
            std::fs::write(&path, &html).with_context(|| format!("write {}", path.display()))?;
            println!("wrote {} ({} bytes)", path.display(), html.len());
        }
        None => print!("{html}"),
    }
    Ok(())
}

#[allow(dead_code)]
fn distinct_communities(d: &VizData) -> usize {
    let mut set: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for n in &d.nodes {
        if n.community >= 0 {
            set.insert(n.community);
        }
    }
    set.len()
}

/// Accumulator interning graph nodes by their identity key while tracking the
/// bare name, definition location, and degree for each.
#[derive(Default)]
struct NodeAcc {
    idx: HashMap<String, usize>,
    keys: Vec<String>,
    names: Vec<String>,
    files: Vec<Option<String>>,
    lines: Vec<Option<u32>>,
    degree: Vec<usize>,
}

impl NodeAcc {
    fn intern(&mut self, key: String, name: &str, file: Option<&str>, line: Option<u32>) -> usize {
        if let Some(&i) = self.idx.get(&key) {
            return i;
        }
        let i = self.keys.len();
        self.idx.insert(key.clone(), i);
        self.keys.push(key);
        self.names.push(name.to_string());
        self.files.push(file.map(str::to_string));
        self.lines.push(line);
        self.degree.push(0);
        i
    }

    /// Finalize: qualify labels for names with >1 definition node, color by
    /// community via the per-key map, and flag major (subsystem) membership.
    fn into_nodes(
        self,
        community_of: &HashMap<&str, i64>,
        major: &std::collections::HashSet<i64>,
    ) -> Vec<VizNode> {
        let mut name_count: HashMap<&str, usize> = HashMap::new();
        for n in &self.names {
            *name_count.entry(n.as_str()).or_insert(0) += 1;
        }
        (0..self.keys.len())
            .map(|i| {
                let name = &self.names[i];
                let label = if name_count.get(name.as_str()).copied().unwrap_or(0) > 1 {
                    match (&self.files[i], self.lines[i]) {
                        // line stored 0-based (Tree-sitter native); show 1-based.
                        (Some(f), Some(l)) => format!("{name}@{}:{}", basename(f), l + 1),
                        _ => format!("{name}@?"),
                    }
                } else {
                    name.clone()
                };
                let community = community_of
                    .get(self.keys[i].as_str())
                    .copied()
                    .unwrap_or(-1);
                VizNode {
                    community,
                    degree: self.degree[i],
                    name: label,
                    major: major.contains(&community),
                }
            })
            .collect()
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, community: i64, major: bool) -> VizNode {
        VizNode {
            name: name.into(),
            community,
            degree: 1,
            major,
        }
    }

    #[test]
    fn major_flag_and_community_split() {
        let nodes = vec![
            node("a", 0, true),
            node("b", 0, true),
            node("c", -1, false), // ambiguous bucket
        ];
        let d = VizData {
            nodes,
            edges: vec![],
        };
        assert_eq!(distinct_communities(&d), 1);
        assert_eq!(d.nodes.iter().filter(|n| n.community < 0).count(), 1);
        assert_eq!(d.nodes.iter().filter(|n| n.major).count(), 2);
    }

    #[test]
    fn into_nodes_flags_major_and_qualifies_collisions() {
        let mut acc = NodeAcc::default();
        // two `dup` defs (collision) + one unique `solo`.
        let a = acc.intern(node_key("dup", "a.ts", 0), "dup", Some("a.ts"), Some(0));
        let b = acc.intern(node_key("dup", "b.ts", 0), "dup", Some("b.ts"), Some(0));
        let c = acc.intern(node_key("solo", "c.ts", 0), "solo", Some("c.ts"), Some(0));
        let mut community_of: HashMap<&str, i64> = HashMap::new();
        let ka = node_key("dup", "a.ts", 0);
        let kc = node_key("solo", "c.ts", 0);
        community_of.insert(&ka, 7);
        community_of.insert(&kc, 9);
        let _ = (a, b, c);
        let major: std::collections::HashSet<i64> = [7].into_iter().collect();
        let nodes = acc.into_nodes(&community_of, &major);
        let find = |n: &str| nodes.iter().find(|x| x.name == n).map(|x| x.major);
        // collision qualified -> "dup@a.ts:1"; community 7 is major.
        assert_eq!(find("dup@a.ts:1"), Some(true));
        assert_eq!(find("dup@b.ts:1"), Some(false), "no community -> not major");
        assert_eq!(find("solo"), Some(false), "community 9 not in major set");
    }
}
