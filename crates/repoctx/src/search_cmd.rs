//! `repoctx search <pattern>` — textually-complete search with provenance
//! (epic f4cb992; provenance + call edges: issue 52a1e2c).
//!
//! Every result is tagged with where the knowledge came from, so the agent
//! knows how much to trust it:
//!   - `structural` — tree-sitter parsed it as a named symbol (kind + range
//!     known). Highest confidence.
//!   - `reference`  — a known symbol appears here in a call position (from the
//!     call-edge sites). Medium.
//!   - `textual`    — substring matched, AST did not confirm. Grep-level.
//!
//! Structural items also carry their `callers`/`callees` — the thing grep
//! categorically cannot do — including unresolved/ambiguous callees (marked,
//! never dropped). repoctx runs real ripgrep under the hood for the textual
//! layer and owns the compression. Lines are 0-based (machine contract);
//! `HumanRender` prints 1-based.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use repoctx_backend::{CallEdge, CodeIntelBackend, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

/// Compression caps — keep token cost far below "open every matching file".
const MAX_FILES: usize = 40;
const MAX_PER_FILE: usize = 8;
const MAX_LINE_LEN: usize = 200;
/// Cap on callers/callees attached to a structural item.
const MAX_CALL_EDGES: usize = 50;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub pattern: String,
    /// Flat, provenance-tagged stream: structural items first, then
    /// references, then textual.
    pub results: Vec<SearchItem>,
    /// True when the textual layer hit a file/line cap.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchItem {
    /// `"structural"` | `"reference"` | `"textual"`.
    pub source: &'static str,
    pub path: String,
    /// 0-based line.
    pub line: u32,
    /// Symbol name (structural items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Symbol kind (structural items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// End line of the symbol range (structural items), 0-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// The matched source line (reference / textual items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Symbols that call this one (structural exact-name items only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<EdgeGroup>,
    /// Symbols this one calls (structural exact-name items only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callees: Option<EdgeGroup>,
}

/// Call edges grouped by how the name resolves **within the indexed scope**
/// (issue cd2680f). `external` = no definition in what we parsed (stdlib /
/// third-party / builtin / uncovered-language file) — NOT "outside the repo".
/// Noise (external + ambiguous) collapses to counts by default; `--all-callees`
/// fills `external`/`candidates`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EdgeGroup {
    /// Resolves to exactly one indexed symbol — the valuable case. Expanded.
    pub internal: Vec<CallRef>,
    /// Resolves to several indexed symbols — collapsed to per-name counts.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ambiguous: Vec<AmbiguousGroup>,
    /// Count of distinct names with no definition in the indexed scope.
    #[serde(skip_serializing_if = "is_zero")]
    pub external_count: usize,
    /// External names — only filled when expanded (`--all-callees`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external: Vec<String>,
}

impl EdgeGroup {
    fn is_empty(&self) -> bool {
        self.internal.is_empty() && self.ambiguous.is_empty() && self.external_count == 0
    }
}

/// A name resolving to multiple indexed symbols: count by default, candidate
/// locations only when expanded.
#[derive(Debug, Clone, Serialize)]
pub struct AmbiguousGroup {
    pub name: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CallRef>,
}

/// An internal (resolved-in-index) edge endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallRef {
    pub name: String,
    pub path: String,
    pub line: u32,
    pub kind: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A raw ripgrep hit, 0-based line.
struct RawMatch {
    path: String,
    line: u32,
    text: String,
}

pub fn run(
    repo_root: &Path,
    pattern: String,
    lang: Option<String>,
    limit: usize,
    all_callees: bool,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);

    // Structural: every symbol whose name contains the query — tree-sitter
    // confirmed these are real symbols (kind + range known), even when the
    // name only contains the substring (e.g. `to_call_edges` for `call_edges`).
    // Each carries its OWN callers/callees, queried by its own name below.
    let q = SymbolQuery {
        query: pattern.clone(),
        kind: None,
        language: lang.clone(),
        limit: 0,
    };
    let mut symbols = backend.workspace_symbols(&q)?;
    let sym_cap = if limit == 0 { usize::MAX } else { limit };
    symbols.truncate(sym_cap);

    // References: call sites of the *queried* name (uses of what you searched).
    // Keyed to the exact query — call sites of a different same-substring
    // symbol stay `textual`.
    let query_caller_edges = backend.callers(&pattern).unwrap_or_default();

    // Textual: every ripgrep occurrence (0-based lines).
    let (raw, files_truncated, rg_ran) = ripgrep(repo_root, &pattern, lang.as_deref(), limit);

    // Classification sets (0-based).
    let struct_locs: HashSet<(&str, u32)> = symbols
        .iter()
        .map(|s| (s.location.path.as_str(), s.location.start_line))
        .collect();
    let ref_sites: HashSet<(&str, u32)> = query_caller_edges
        .iter()
        .map(|e| (e.site.path.as_str(), e.site.start_line))
        .collect();

    // Per-structural-symbol call edges, by that symbol's own name (memoized —
    // every structural result gets its own neighborhood, not just the exact
    // query match). The grouping collapses external/ambiguous noise, so each
    // stays cheap.
    let mut edge_cache: std::collections::HashMap<String, (Option<EdgeGroup>, Option<EdgeGroup>)> =
        std::collections::HashMap::new();

    let mut results: Vec<SearchItem> = Vec::new();
    for s in &symbols {
        let (callers, callees) = edge_cache
            .entry(s.name.clone())
            .or_insert_with(|| {
                let ce = backend.callers(&s.name).unwrap_or_default();
                let ee = backend.callees(&s.name).unwrap_or_default();
                (callers_group(&ce), callees_group(&ee, all_callees))
            })
            .clone();
        results.push(SearchItem {
            source: "structural",
            path: s.location.path.clone(),
            line: s.location.start_line,
            name: Some(s.name.clone()),
            kind: Some(s.kind.as_str().to_string()),
            end_line: Some(s.location.end_line),
            text: None,
            callers,
            callees,
        });
    }
    // Then references, then textual (skip lines already shown as structural).
    let mut refs: Vec<SearchItem> = Vec::new();
    let mut texts: Vec<SearchItem> = Vec::new();
    for m in raw {
        if struct_locs.contains(&(m.path.as_str(), m.line)) {
            continue;
        }
        let is_ref = ref_sites.contains(&(m.path.as_str(), m.line));
        let item = SearchItem {
            source: if is_ref { "reference" } else { "textual" },
            path: m.path,
            line: m.line,
            name: None,
            kind: None,
            end_line: None,
            text: Some(m.text),
            callers: None,
            callees: None,
        };
        if is_ref {
            refs.push(item);
        } else {
            texts.push(item);
        }
    }
    results.append(&mut refs);
    results.append(&mut texts);

    let advisory = advisory(files_truncated, rg_ran);
    let candidate_paths = candidate_paths(&results);
    let result = SearchResult {
        pattern: pattern.clone(),
        results,
        truncated: files_truncated,
        advisory,
    };

    let mut store = backend.into_store();
    crate::gain::emit_and_record(
        &result,
        render,
        &mut store,
        gain_opts,
        "search",
        Some(pattern.as_str()),
        &candidate_paths,
    )
}

fn call_ref(sym: &repoctx_backend::Symbol) -> CallRef {
    CallRef {
        name: sym.name.clone(),
        path: sym.location.path.clone(),
        line: sym.location.start_line,
        kind: sym.kind.as_str().to_string(),
    }
}

/// Callers: who calls the query. The caller is always a resolved indexed
/// symbol, so every caller is `internal`. Returns `None` when empty.
fn callers_group(edges: &[CallEdge]) -> Option<EdgeGroup> {
    let mut internal: Vec<CallRef> = Vec::new();
    for e in edges {
        let r = call_ref(&e.caller);
        if !internal.contains(&r) {
            internal.push(r);
        }
        if internal.len() >= MAX_CALL_EDGES {
            break;
        }
    }
    let g = EdgeGroup {
        internal,
        ..Default::default()
    };
    (!g.is_empty()).then_some(g)
}

/// Callees: what the query calls, grouped by resolution within the indexed
/// scope. `internal` (one indexed def) is expanded; `internal-ambiguous`
/// (several indexed defs) and `external` (no indexed def — stdlib/3rd-party/
/// builtin) collapse to counts unless `expand` is set. Returns `None` when
/// empty.
fn callees_group(edges: &[CallEdge], expand: bool) -> Option<EdgeGroup> {
    use std::collections::BTreeMap;
    let mut internal: Vec<CallRef> = Vec::new();
    let mut ambiguous: BTreeMap<String, Vec<CallRef>> = BTreeMap::new();
    let mut external: Vec<String> = Vec::new();
    for e in edges {
        match &e.callee {
            // No definition in the indexed scope → external (expected; not a
            // failure). Defined by what we parsed, not the repo boundary.
            None => {
                if !external.contains(&e.callee_name) {
                    external.push(e.callee_name.clone());
                }
            }
            // Resolves to several indexed symbols → collapse per name.
            Some(sym) if e.ambiguous => {
                let cands = ambiguous.entry(e.callee_name.clone()).or_default();
                let r = call_ref(sym);
                if !cands.contains(&r) {
                    cands.push(r);
                }
            }
            // Exactly one indexed symbol → the valuable case.
            Some(sym) => {
                let r = call_ref(sym);
                if !internal.contains(&r) {
                    internal.push(r);
                }
            }
        }
    }
    internal.truncate(MAX_CALL_EDGES);
    let ambiguous: Vec<AmbiguousGroup> = ambiguous
        .into_iter()
        .map(|(name, candidates)| AmbiguousGroup {
            name,
            count: candidates.len(),
            candidates: if expand { candidates } else { Vec::new() },
        })
        .collect();
    let external_count = external.len();
    let g = EdgeGroup {
        internal,
        ambiguous,
        external_count,
        external: if expand {
            let mut e = external;
            e.sort();
            e
        } else {
            Vec::new()
        },
    };
    (!g.is_empty()).then_some(g)
}

/// Run ripgrep in the repo, parse `path:line:text` into 0-based `RawMatch`es.
/// Returns (matches, files_truncated, rg_ran). Never errors: a missing
/// ripgrep degrades to empty + `rg_ran = false`.
fn ripgrep(
    repo_root: &Path,
    pattern: &str,
    lang: Option<&str>,
    limit: usize,
) -> (Vec<RawMatch>, bool, bool) {
    let max_files = if limit == 0 {
        MAX_FILES
    } else {
        limit.min(MAX_FILES)
    };
    let mut cmd = Command::new("rg");
    cmd.current_dir(repo_root)
        .arg("--no-heading")
        .arg("--line-number")
        .arg("--color=never")
        .arg("-m")
        .arg(MAX_PER_FILE.to_string());
    if let Some(t) = lang.and_then(rg_type) {
        cmd.arg("--type").arg(t);
    }
    cmd.arg("--").arg(pattern);

    let out = match cmd.output() {
        Ok(o) => o,
        Err(_) => return (Vec::new(), false, false),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut matches: Vec<RawMatch> = Vec::new();
    let mut seen_files: Vec<String> = Vec::new();
    let mut files_truncated = false;
    for line in stdout.lines() {
        let Some((path, rest)) = line.split_once(':') else {
            continue;
        };
        let Some((lineno, text)) = rest.split_once(':') else {
            continue;
        };
        let Ok(lineno) = lineno.parse::<u32>() else {
            continue;
        };
        if !seen_files.iter().any(|p| p == path) {
            if seen_files.len() >= max_files {
                files_truncated = true;
                continue;
            }
            seen_files.push(path.to_string());
        }
        matches.push(RawMatch {
            path: path.to_string(),
            // ripgrep is 1-based; store 0-based to match the symbol contract.
            line: lineno.saturating_sub(1),
            text: truncate(text.trim_end(), MAX_LINE_LEN),
        });
    }
    (matches, files_truncated, true)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    t.push('…');
    t
}

fn advisory(files_truncated: bool, rg_ran: bool) -> Option<String> {
    if !rg_ran {
        return Some(
            "ripgrep not on PATH — textual matches unavailable; showing \
             structural results only. Install ripgrep for complete results."
                .into(),
        );
    }
    if files_truncated {
        return Some(format!(
            "textual results truncated (caps: {MAX_FILES} files, \
             {MAX_PER_FILE}/file) — narrow the pattern or run `rg` directly"
        ));
    }
    None
}

fn candidate_paths(results: &[SearchItem]) -> Vec<String> {
    let mut out: Vec<String> = results.iter().map(|r| r.path.clone()).collect();
    out.sort();
    out.dedup();
    out
}

/// repoctx language slug → ripgrep `--type` name. None → don't constrain by type.
fn rg_type(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "rust" => "rust",
        "go" => "go",
        "python" => "py",
        "javascript" => "js",
        "typescript" | "tsx" => "ts",
        "c" => "c",
        "cpp" => "cpp",
        "java" => "java",
        "ruby" => "ruby",
        "csharp" => "cs",
        "php" => "php",
        "lua" => "lua",
        "kotlin" => "kotlin",
        "swift" => "swift",
        "json" => "json",
        "yaml" => "yaml",
        "toml" => "toml",
        "markdown" => "md",
        "bash" => "sh",
        _ => return None,
    })
}

impl HumanRender for SearchResult {
    fn human(&self) -> String {
        let mut out = String::new();
        let structural: Vec<&SearchItem> = self
            .results
            .iter()
            .filter(|r| r.source == "structural")
            .collect();
        let refs: Vec<&SearchItem> = self
            .results
            .iter()
            .filter(|r| r.source == "reference")
            .collect();
        let texts: Vec<&SearchItem> = self
            .results
            .iter()
            .filter(|r| r.source == "textual")
            .collect();

        if !structural.is_empty() {
            out.push_str("structural:\n");
            for s in &structural {
                out.push_str(&format!(
                    "  {}:{}  {}  {}\n",
                    s.path,
                    s.line + 1,
                    s.name.as_deref().unwrap_or(""),
                    s.kind.as_deref().unwrap_or("")
                ));
                if let Some(g) = &s.callers {
                    render_edge_group(&mut out, "caller", g);
                }
                if let Some(g) = &s.callees {
                    render_edge_group(&mut out, "callee", g);
                }
            }
        }
        if !refs.is_empty() {
            out.push_str("references (call sites):\n");
            for r in &refs {
                out.push_str(&format!(
                    "  {}:{}  {}\n",
                    r.path,
                    r.line + 1,
                    r.text.as_deref().unwrap_or("")
                ));
            }
        }
        if !texts.is_empty() {
            out.push_str("textual:\n");
            for t in &texts {
                out.push_str(&format!(
                    "  {}:{}  {}\n",
                    t.path,
                    t.line + 1,
                    t.text.as_deref().unwrap_or("")
                ));
            }
        }
        if structural.is_empty() && refs.is_empty() && texts.is_empty() {
            out.push_str("no results");
        }
        if let Some(a) = &self.advisory {
            out.push_str("\nadvisory: ");
            out.push_str(a);
        }
        out.trim_end().to_string()
    }
}

fn render_edge_group(out: &mut String, label: &str, g: &EdgeGroup) {
    for c in &g.internal {
        out.push_str(&format!(
            "    {label}: {}  {}:{}  {}\n",
            c.name,
            c.path,
            c.line + 1,
            c.kind
        ));
    }
    for a in &g.ambiguous {
        out.push_str(&format!(
            "    {label}: {}  {} internal candidates\n",
            a.name, a.count
        ));
        for c in &a.candidates {
            out.push_str(&format!("      - {}:{}  {}\n", c.path, c.line + 1, c.kind));
        }
    }
    if g.external_count > 0 {
        out.push_str(&format!("    {label}s: {} external\n", g.external_count));
        if !g.external.is_empty() {
            out.push_str(&format!("      ({})\n", g.external.join(", ")));
        }
    }
}
