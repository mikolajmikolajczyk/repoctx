//! `repoctx changed [--since REF]` — change-aware blast radius (issue #6).
//!
//! git diff → which **symbols** overlap the changed lines → their transitive
//! **callers** = "what this change touches + what it can break". Pairs with
//! code review: answers the reviewer's first question structurally instead of
//! reading the whole diff.
//!
//! Caller traversal reuses the call graph's narrow `backend.callers` queries
//! (plain reverse BFS, name-based per ADR-0010) — not petgraph: the
//! petgraph-for-graph-algos decision targets SCC/toposort/centrality, not a
//! reverse-reachability walk we already do narrowly.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use repoctx_backend::{CodeIntelBackend, TreeSitterBackend};
use repoctx_store::Store;
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

/// Cap on the blast-radius walk so a hot changed symbol can't explode output.
const MAX_IMPACTED: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct ChangedSymbol {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Impacted {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
    /// BFS depth from a changed symbol (1 = direct caller).
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedReport {
    pub since: String,
    pub files_changed: usize,
    pub changed: Vec<ChangedSymbol>,
    /// Transitive callers of the changed symbols — the blast radius.
    pub impacted: Vec<Impacted>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for ChangedReport {
    fn human(&self) -> String {
        let mut s = format!(
            "changed since {} — {} file(s), {} symbol(s) changed, {} impacted\n",
            self.since,
            self.files_changed,
            self.changed.len(),
            self.impacted.len()
        );
        if !self.changed.is_empty() {
            s.push_str("\n## changed symbols\n");
            for c in &self.changed {
                s.push_str(&format!(
                    "  {}:{}  {} {}\n",
                    c.path,
                    c.line + 1,
                    c.name,
                    c.kind
                ));
            }
        }
        if !self.impacted.is_empty() {
            s.push_str("\n## blast radius (transitive callers)\n");
            for i in &self.impacted {
                s.push_str(&format!(
                    "  [d{}] {}:{}  {} {}\n",
                    i.depth,
                    i.path,
                    i.line + 1,
                    i.name,
                    i.kind
                ));
            }
        }
        if let Some(a) = &self.advisory {
            s.push_str("\nadvisory: ");
            s.push_str(a);
        }
        s.trim_end().to_string()
    }
}

/// One changed line range on the new side of a hunk (1-based, inclusive).
struct Hunk {
    path: String,
    start: u32,
    end: u32,
}

/// Parse `git diff --unified=0 <ref>` into new-side changed line ranges per
/// file. Tracks the `+++ b/<path>` header and `@@ … +start,count @@` hunks.
fn parse_diff(diff: &str) -> Vec<Hunk> {
    let mut out = Vec::new();
    let mut cur_path: Option<String> = None;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            // "+++ b/path" or "+++ /dev/null" (deletion — no new side).
            cur_path = rest
                .strip_prefix("b/")
                .filter(|p| *p != "/dev/null")
                .map(|p| p.to_string());
        } else if line.starts_with("@@ ") {
            // @@ -a,b +c,d @@ ...
            if let (Some(path), Some(plus)) = (
                cur_path.as_ref(),
                line.split(' ').find(|t| t.starts_with('+')),
            ) {
                let nums = &plus[1..];
                let (start, count) = match nums.split_once(',') {
                    Some((s, c)) => (s.parse().unwrap_or(0), c.parse().unwrap_or(1)),
                    None => (nums.parse().unwrap_or(0), 1u32),
                };
                if start > 0 && count > 0 {
                    out.push(Hunk {
                        path: path.clone(),
                        start,
                        end: start + count - 1,
                    });
                }
            }
        }
    }
    out
}

fn git_diff(repo_root: &Path, since: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--unified=0", "--no-color", since])
        .output()
        .context("run git diff")?;
    if !out.status.success() {
        anyhow::bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn run(repo_root: &Path, since: String, render: Render, gain_opts: GainOpts) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let diff = git_diff(repo_root, &since)?;
    let hunks = parse_diff(&diff);

    let mut files: HashSet<String> = HashSet::new();
    for h in &hunks {
        files.insert(h.path.clone());
    }

    let store = Store::open(repo_root).context("open store")?;

    // Changed symbols: indexed symbols whose [start,end] (0-based) overlap any
    // changed hunk line (1-based git → 0-based compare).
    let mut changed: Vec<ChangedSymbol> = Vec::new();
    let mut seen_changed: HashSet<(String, String, u32)> = HashSet::new();
    for path in &files {
        let syms = store.symbols_by_file(path).unwrap_or_default();
        for s in syms {
            let overlaps = hunks
                .iter()
                .any(|h| h.path == *path && h.start <= s.end_line + 1 && h.end > s.start_line);
            if overlaps && seen_changed.insert((s.name.clone(), path.clone(), s.start_line)) {
                changed.push(ChangedSymbol {
                    name: s.name,
                    kind: s.kind,
                    path: path.clone(),
                    line: s.start_line,
                });
            }
        }
    }

    // Blast radius: reverse BFS over callers of the changed symbol names.
    let backend = TreeSitterBackend::new(store);
    let mut visited: HashSet<String> = changed.iter().map(|c| c.name.clone()).collect();
    let mut frontier: Vec<String> = changed.iter().map(|c| c.name.clone()).collect();
    let mut impacted: Vec<Impacted> = Vec::new();
    let mut seen_imp: HashSet<(String, String, u32)> = HashSet::new();
    let mut truncated = false;
    let mut depth = 1u32;
    'outer: while !frontier.is_empty() {
        let mut next: Vec<String> = Vec::new();
        for sym in &frontier {
            for e in backend.callers(sym)? {
                let c = e.caller;
                let key = (
                    c.name.clone(),
                    c.location.path.clone(),
                    c.location.start_line,
                );
                if seen_imp.insert(key) {
                    impacted.push(Impacted {
                        name: c.name.clone(),
                        kind: c.kind.as_str().to_string(),
                        path: c.location.path,
                        line: c.location.start_line,
                        depth,
                    });
                    if impacted.len() >= MAX_IMPACTED {
                        truncated = true;
                        break 'outer;
                    }
                }
                if visited.insert(c.name.clone()) {
                    next.push(c.name);
                }
            }
        }
        frontier = next;
        depth += 1;
    }

    let advisory = advisory(&since, files.len(), changed.is_empty(), truncated);
    let mut candidate_paths: Vec<String> = changed
        .iter()
        .map(|c| c.path.clone())
        .chain(impacted.iter().map(|i| i.path.clone()))
        .collect();
    candidate_paths.sort();
    candidate_paths.dedup();
    let report = ChangedReport {
        since,
        files_changed: files.len(),
        changed,
        impacted,
        advisory,
    };
    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &report,
        render,
        &mut store,
        gain_opts,
        "changed",
        None,
        &candidate_paths,
    )
}

fn advisory(since: &str, files: usize, no_symbols: bool, truncated: bool) -> Option<String> {
    if truncated {
        return Some(format!(
            "blast radius capped at {MAX_IMPACTED} impacted symbols"
        ));
    }
    if files == 0 {
        return Some(format!(
            "no changes vs `{since}` (tracked files only — untracked files aren't in `git diff`)"
        ));
    }
    if no_symbols {
        return Some(
            "changed lines don't overlap any indexed symbol (comments/imports/data, or a \
             language without symbol coverage)"
                .to_string(),
        );
    }
    Some(
        "blast radius is name-based (ADR-0010): dynamic dispatch / traits / external callers \
         invisible; verify before relying on it"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::parse_diff;

    #[test]
    fn parses_new_side_hunks() {
        let diff = "\
diff --git a/src/a.ts b/src/a.ts
--- a/src/a.ts
+++ b/src/a.ts
@@ -10,2 +10,3 @@ fn x()
+line
@@ -40 +41 @@
-old
+new
";
        let h = parse_diff(diff);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].path, "src/a.ts");
        assert_eq!((h[0].start, h[0].end), (10, 12)); // +10,3
        assert_eq!((h[1].start, h[1].end), (41, 41)); // +41 (count omitted = 1)
    }

    #[test]
    fn deletion_has_no_new_side() {
        let diff = "\
diff --git a/gone.ts b/gone.ts
--- a/gone.ts
+++ /dev/null
@@ -1,3 +0,0 @@
-a
";
        // +0,0 -> start 0 filtered out; /dev/null path filtered.
        assert!(parse_diff(diff).is_empty());
    }
}
