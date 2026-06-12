//! `HumanRender` impl for symbol lists.
//!
//! Aligned columns `path:line  name  kind`. Lines are rendered 1-based for
//! humans; DB and machine output stay 0-based per the contract.

use repoctx_backend::Symbol;

use crate::output::{HumanRender, List};

impl HumanRender for List<Symbol> {
    fn human(&self) -> String {
        let mut out = if self.items.is_empty() {
            "no symbols".to_string()
        } else {
            let rows: Vec<(String, &str, &str)> = self
                .items
                .iter()
                .map(|s| {
                    let loc = format!("{}:{}", s.location.path, s.location.start_line + 1);
                    (loc, s.name.as_str(), s.kind.as_str())
                })
                .collect();
            let w_loc = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
            let w_name = rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
            let mut s = String::new();
            for (i, (loc, name, kind)) in rows.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                s.push_str(&format!("{loc:<w_loc$}  {name:<w_name$}  {kind}"));
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
