//! `HumanRender` impl for call-edge lists.
//!
//! Columns: `caller_loc  caller -> callee  callee_loc [flags]`. Lines are
//! 1-based for humans; machine output stays 0-based per the contract.

use repoctx_backend::CallEdge;

use crate::callgraph_cmd::GraphEdge;
use crate::output::{HumanRender, List};

impl HumanRender for List<CallEdge> {
    fn human(&self) -> String {
        let mut out = if self.items.is_empty() {
            "no call edges".to_string()
        } else {
            let rows: Vec<(String, String, String)> = self
                .items
                .iter()
                .map(|e| {
                    let caller_loc = format!(
                        "{}:{}",
                        e.caller.location.path,
                        e.caller.location.start_line + 1
                    );
                    let edge = format!("{} -> {}", e.caller.name, e.callee_name);
                    let callee = match &e.callee {
                        Some(c) => format!("{}:{}", c.location.path, c.location.start_line + 1),
                        None => "<external>".to_string(),
                    };
                    let mut tail = String::new();
                    if e.ambiguous {
                        tail.push_str(" [ambiguous]");
                    }
                    if e.resolution != "syntactic" {
                        tail.push_str(" (semantic)");
                    }
                    (caller_loc, edge, format!("{callee}{tail}"))
                })
                .collect();
            let w0 = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
            let w1 = rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
            let mut s = String::new();
            for (i, (a, b, c)) in rows.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                s.push_str(&format!("{a:<w0$}  {b:<w1$}  {c}"));
            }
            s
        };
        if let Some(a) = &self.advisory {
            out.push_str("\n\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}

impl HumanRender for List<GraphEdge> {
    fn human(&self) -> String {
        let mut out = if self.items.is_empty() {
            "no call edges".to_string()
        } else {
            let mut s = String::new();
            for (i, g) in self.items.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                let indent = "  ".repeat(g.depth.saturating_sub(1) as usize);
                let callee = match &g.edge.callee {
                    Some(c) => format!("{}:{}", c.location.path, c.location.start_line + 1),
                    None => "<external>".to_string(),
                };
                let mut tail = String::new();
                if g.edge.ambiguous {
                    tail.push_str(" [ambiguous]");
                }
                if g.edge.resolution != "syntactic" {
                    tail.push_str(" (semantic)");
                }
                s.push_str(&format!(
                    "{indent}[{}{}] {} -> {}  {callee}{tail}",
                    g.direction.chars().next().unwrap_or('?'),
                    g.depth,
                    g.edge.caller.name,
                    g.edge.callee_name,
                ));
            }
            s
        };
        if let Some(a) = &self.advisory {
            out.push_str("\n\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}
