//! `repoctx outline <file>` — document symbols for one file as a flat
//! machine record or an indented tree for humans.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use repoctx_backend::{CodeIntelBackend, Symbol, TreeSitterBackend};
use repoctx_store::{to_db_path, Store};
use serde::Serialize;

use crate::gain::{GainOpts, Recorder};
use crate::output::{HumanRender, Render};
use crate::read_cmd;

#[derive(Debug, Serialize)]
pub struct OutlineReport {
    #[serde(skip)]
    pub file: String,
    pub count: usize,
    pub items: Vec<Symbol>,
}

impl HumanRender for OutlineReport {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n", self.file));
        if self.items.is_empty() {
            out.push_str("(no symbols)");
            return out;
        }
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

#[allow(clippy::too_many_arguments)]
pub fn run(
    repo_root: &Path,
    file_arg: PathBuf,
    render: Render,
    gain_opts: GainOpts,
    no_auto_index: bool,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root, no_auto_index)?;
    let db_path = normalize_path(repo_root, &file_arg)?;

    let store = Store::open(repo_root).context("open store")?;
    if !store.file_exists(&db_path).context("file_exists")? {
        bail!(
            "{} is not in the index — file may be new, ignored, oversized (>2 MiB), \
             non-UTF-8, or in an unsupported language. Run `repoctx index` to refresh.",
            db_path
        );
    }
    let backend = TreeSitterBackend::new(store);
    let items = backend.document_symbols(&PathBuf::from(&db_path))?;

    let report = OutlineReport {
        file: db_path.clone(),
        count: items.len(),
        items,
    };

    let mut buf = Vec::new();
    crate::output::emit_to(&mut buf, &report, render)?;
    std::io::Write::write_all(&mut std::io::stdout().lock(), &buf)?;

    let rendered = String::from_utf8_lossy(&buf).into_owned();
    let mut store = backend.into_store();
    let mut recorder = Recorder::new(&mut store, gain_opts);
    recorder.record(
        "outline",
        Some(db_path.as_str()),
        std::slice::from_ref(&db_path),
        &rendered,
        render.name(),
    );
    Ok(())
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
