//! `repoctx context <symbol>` — exact-name lookup plus a window of
//! surrounding source for each hit. Composite command: agents get
//! "where + what" in one call.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use repoctx_backend::{CodeIntelBackend, Location, Symbol, SymbolQuery, TreeSitterBackend};
use repoctx_store::{from_db_path, Store};
use serde::Serialize;
use tracing::warn;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

#[derive(Debug, Serialize)]
pub struct ContextMatch {
    pub symbol: String,
    pub kind: String,
    pub location: Location,
    pub before: String,
    pub body: String,
    pub after: String,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
pub struct ContextReport {
    pub count: usize,
    pub items: Vec<ContextMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for ContextReport {
    fn human(&self) -> String {
        if self.items.is_empty() {
            let mut out = String::from("no matches");
            if let Some(a) = &self.advisory {
                out.push_str("\n\nadvisory: ");
                out.push_str(a);
            }
            return out;
        }
        let mut out = String::new();
        for (i, m) in self.items.iter().enumerate() {
            if i > 0 {
                out.push_str("\n---\n");
            }
            out.push_str(&format!(
                "# {}:{}  {}  {}\n",
                m.location.path,
                m.location.start_line + 1,
                m.symbol,
                m.kind,
            ));
            if m.stale {
                out.push_str("(stale — file changed since last index)\n");
            }
            // Concatenate the three slices with explicit newline separators
            // so a single empty-line `before` doesn't collapse to zero
            // visible lines (lines() drops trailing empties).
            let mut joined = String::new();
            for slice in [&m.before, &m.body, &m.after] {
                if !slice.is_empty() {
                    if !joined.is_empty() {
                        joined.push('\n');
                    }
                    joined.push_str(slice);
                }
            }
            // Number lines from the start of the window.
            let before_lines = if m.before.is_empty() {
                0
            } else {
                m.before.split('\n').count()
            };
            let first = (m.location.start_line as usize + 1).saturating_sub(before_lines);
            for (line_no, line) in (first..).zip(joined.split('\n')) {
                out.push_str(&format!("{line_no:>5}  {line}\n"));
            }
        }
        // strip trailing newline so caller adds exactly one.
        if out.ends_with('\n') {
            out.pop();
        }
        if let Some(a) = &self.advisory {
            out.push_str("\n\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}

pub fn run(
    repo_root: &Path,
    symbol: String,
    context: usize,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);

    let q = SymbolQuery {
        query: symbol.clone(),
        kind: None,
        language: None,
        limit: 0,
    };
    let mut hits: Vec<Symbol> = backend
        .workspace_symbols(&q)?
        .into_iter()
        .filter(|s| s.name == symbol)
        .collect();
    // Stable rank: shorter file path first, then start_line, then start_column.
    hits.sort_by(|a, b| {
        (
            a.location.path.len(),
            &a.location.path,
            a.location.start_line,
            a.location.start_column,
        )
            .cmp(&(
                b.location.path.len(),
                &b.location.path,
                b.location.start_line,
                b.location.start_column,
            ))
    });
    hits.truncate(limit);

    let advisory = crate::definition_cmd::compute_advisory(&backend, None, &symbol, hits.len())?;
    let store = backend.into_store();

    let mut items: Vec<ContextMatch> = Vec::with_capacity(hits.len());
    let mut candidate_paths: Vec<String> = Vec::with_capacity(hits.len());
    for sym in &hits {
        let abs = repo_root.join(from_db_path(&sym.location.path));
        let source = match fs::read_to_string(&abs) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    path = %sym.location.path,
                    error = %e,
                    "context: file unreadable, skipping match",
                );
                continue;
            }
        };
        let stale = is_stale(&store, &abs, &sym.location.path)?;
        let (before, body, after) = slice_window(&source, sym, context);
        candidate_paths.push(sym.location.path.clone());
        items.push(ContextMatch {
            symbol: sym.name.clone(),
            kind: sym.kind.as_str().to_string(),
            location: sym.location.clone(),
            before,
            body,
            after,
            stale,
        });
    }

    let report = ContextReport {
        count: items.len(),
        items,
        advisory,
    };

    candidate_paths.sort();
    candidate_paths.dedup();
    let mut store = store;
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "context",
        Some(symbol.as_str()),
        &candidate_paths,
    )
}

fn is_stale(store: &Store, abs: &Path, db_path: &str) -> Result<bool> {
    let Some((indexed_mtime, indexed_size)) = store.file_stat(db_path).context("file_stat")? else {
        return Ok(false);
    };
    let meta = match fs::metadata(abs) {
        Ok(m) => m,
        Err(_) => return Ok(true),
    };
    // No mtime support → treat as 0; differs from the indexed tuple, so
    // the match is reported `stale` (conservative — never claims fresh).
    let cur_mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    let cur_size = meta.len() as i64;
    Ok(cur_mtime != indexed_mtime || cur_size != indexed_size)
}

/// Split source into `(before, body, after)`. Lines split on `\n`; trailing
/// empty entry from a final newline is dropped so we don't render an empty
/// row. `context` is the number of `before`/`after` lines, clamped to file
/// bounds.
fn slice_window(source: &str, sym: &Symbol, context: usize) -> (String, String, String) {
    let lines: Vec<&str> = source.lines().collect();
    let total = lines.len();
    if total == 0 {
        return (String::new(), String::new(), String::new());
    }
    let start = sym.location.start_line as usize;
    let end = (sym.location.end_line as usize).min(total.saturating_sub(1));
    let before_start = start.saturating_sub(context);
    let after_end = (end + context).min(total.saturating_sub(1));

    let before = join_range(&lines, before_start, start);
    let body = join_range(&lines, start, end + 1);
    let after = join_range(&lines, end + 1, after_end + 1);
    (before, body, after)
}

fn join_range(lines: &[&str], start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    lines[start..end.min(lines.len())].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use repoctx_backend::SymbolKind;

    fn sym(start: u32, end: u32) -> Symbol {
        Symbol {
            name: "f".into(),
            kind: SymbolKind::Function,
            location: Location {
                path: "x".into(),
                start_line: start,
                start_column: 0,
                end_line: end,
                end_column: 0,
            },
        }
    }

    #[test]
    fn window_clamps_at_start() {
        let src = "L0\nL1\nL2\nL3\nL4\n";
        let (b, body, a) = slice_window(src, &sym(0, 1), 3);
        assert_eq!(b, "");
        assert_eq!(body, "L0\nL1");
        assert_eq!(a, "L2\nL3\nL4");
    }

    #[test]
    fn window_clamps_at_end() {
        let src = "L0\nL1\nL2\nL3\nL4";
        let (b, body, a) = slice_window(src, &sym(3, 4), 5);
        assert_eq!(b, "L0\nL1\nL2");
        assert_eq!(body, "L3\nL4");
        assert_eq!(a, "");
    }

    #[test]
    fn window_zero_context() {
        let src = "L0\nL1\nL2\nL3\nL4";
        let (b, body, a) = slice_window(src, &sym(2, 2), 0);
        assert_eq!(b, "");
        assert_eq!(body, "L2");
        assert_eq!(a, "");
    }
}
