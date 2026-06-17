//! `repoctx outline <file>` — document symbols for one file as a flat
//! machine record or an indented tree for humans.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use repoctx_backend::{CodeIntelBackend, Symbol, TreeSitterBackend};
use repoctx_store::{to_db_path, Store};
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

#[derive(Debug, Serialize)]
pub struct OutlineReport {
    #[serde(skip)]
    pub file: String,
    pub count: usize,
    pub items: Vec<Symbol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for OutlineReport {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n", self.file));
        if self.items.is_empty() {
            out.push_str("(no symbols)");
        } else {
            let depths = compute_depths(&self.items);
            for (i, sym) in self.items.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                let indent = "  ".repeat(depths[i]);
                out.push_str(&format!(
                    "{}{}:{}  {}  {}",
                    indent,
                    sym.location.path,
                    sym.location.start_line + 1,
                    sym.name,
                    sym.kind.as_str(),
                ));
            }
        }
        if let Some(a) = &self.advisory {
            out.push_str("\n\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}

/// Depth of each symbol assuming a containment tree.
///
/// Input is ordered by `(start_line, start_column)`. We walk a stack of
/// open ancestors, popping any whose range does not strictly contain the
/// current symbol.
fn compute_depths(items: &[Symbol]) -> Vec<usize> {
    let mut depths = Vec::with_capacity(items.len());
    let mut stack: Vec<usize> = Vec::new();
    for (i, sym) in items.iter().enumerate() {
        while let Some(&top) = stack.last() {
            if contains(&items[top], sym) {
                break;
            }
            stack.pop();
        }
        depths.push(stack.len());
        stack.push(i);
    }
    depths
}

fn contains(outer: &Symbol, inner: &Symbol) -> bool {
    let (os, ie) = (&outer.location, &inner.location);
    let starts_at_or_before = (os.start_line, os.start_column) <= (ie.start_line, ie.start_column);
    let ends_at_or_after = (os.end_line, os.end_column) >= (ie.end_line, ie.end_column);
    let strictly_different = (os.start_line, os.start_column, os.end_line, os.end_column)
        != (ie.start_line, ie.start_column, ie.end_line, ie.end_column);
    starts_at_or_before && ends_at_or_after && strictly_different
}

/// Resolve a CLI path argument (relative or absolute) to its repo-relative
/// DB form. Returns `None` if the path escapes the repo root.
pub fn normalize_path(repo_root: &Path, arg: &Path) -> Result<String> {
    let absolute: PathBuf = if arg.is_absolute() {
        arg.to_path_buf()
    } else {
        std::env::current_dir().context("current dir")?.join(arg)
    };
    let canon_root = repo_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", repo_root.display()))?;
    let canon_arg = match absolute.canonicalize() {
        Ok(p) => p,
        // File missing on disk — fall back to logical join so the index
        // lookup still works for previously-indexed-then-deleted files.
        Err(_) => normalize_logical(&absolute),
    };
    let rel = canon_arg
        .strip_prefix(&canon_root)
        .map_err(|_| anyhow::anyhow!("path is outside repo: {}", arg.display()))?;
    Ok(to_db_path(rel))
}

fn normalize_logical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            ParentDir => {
                out.pop();
            }
            CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn run(repo_root: &Path, file_arg: PathBuf, render: Render, gain_opts: GainOpts) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let db_path = normalize_path(repo_root, &file_arg)?;

    let store = Store::open(repo_root).context("open store")?;
    if !store.file_exists(&db_path).context("file_exists")? {
        // The index is already fresh here (ensure_fresh ran above). Split the
        // two very different causes: a path that doesn't exist (usually a
        // *guessed* path for a symbol) vs a real file that isn't indexable.
        if !repo_root.join(&db_path).is_file() {
            bail!(
                "no such file: {db_path}\n\
                 If you're looking for a symbol (not a file), find where it lives first:\n\
                 \x20 repoctx definition <name>   (exact-name definition + its file)\n\
                 \x20 repoctx search <name>       (defs + every textual match)\n\
                 then `repoctx outline <that-file>`."
            );
        }
        bail!(
            "{db_path} exists but isn't indexed — gitignored, oversized (>2 MiB), \
             non-UTF-8, or an unsupported language (see `repoctx languages`)."
        );
    }
    let backend = TreeSitterBackend::new(store);
    let items = backend.document_symbols(&PathBuf::from(&db_path))?;

    let advisory = crate::advisory::for_file(&db_path);
    let report = OutlineReport {
        file: db_path.clone(),
        count: items.len(),
        items,
        advisory,
    };

    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "outline",
        Some(db_path.as_str()),
        std::slice::from_ref(&db_path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use repoctx_backend::{Location, Symbol, SymbolKind};

    fn sym(name: &str, s: (u32, u32), e: (u32, u32)) -> Symbol {
        Symbol {
            name: name.into(),
            kind: SymbolKind::Other,
            location: Location {
                path: "f".into(),
                start_line: s.0,
                start_column: s.1,
                end_line: e.0,
                end_column: e.1,
            },
        }
    }

    #[test]
    fn depths_nested_class_with_methods() {
        // class Outer 0..30, method m1 2..6, method m2 8..14 (nested fn inner 10..12),
        // sibling free function 32..40.
        let items = vec![
            sym("Outer", (0, 0), (30, 1)),
            sym("m1", (2, 4), (6, 5)),
            sym("m2", (8, 4), (14, 5)),
            sym("inner", (10, 8), (12, 9)),
            sym("free", (32, 0), (40, 1)),
        ];
        assert_eq!(compute_depths(&items), vec![0, 1, 1, 2, 0]);
    }

    #[test]
    fn depths_no_nesting_when_disjoint() {
        let items = vec![
            sym("a", (0, 0), (5, 0)),
            sym("b", (6, 0), (10, 0)),
            sym("c", (11, 0), (15, 0)),
        ];
        assert_eq!(compute_depths(&items), vec![0, 0, 0]);
    }
}
