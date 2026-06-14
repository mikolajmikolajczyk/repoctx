//! `repoctx search <pattern>` — textually-complete search (epic f4cb992).
//!
//! Leads with the symbol definitions named `<pattern>` (the likely target),
//! then lists *every* textual occurrence ripgrep finds (comments, strings,
//! anything), compressed to `file:line` with per-file/line caps. This is the
//! no-textual-loss complement to `symbols`/`definition`: repoctx runs real
//! ripgrep under the hood and owns the compression (correctly), instead of
//! leaving flagged `rg` to the rtk chain.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use repoctx_backend::{CodeIntelBackend, Symbol, SymbolQuery, TreeSitterBackend};
use repoctx_store::Store;
use serde::Serialize;

use crate::gain::GainOpts;
use crate::output::{HumanRender, Render};
use crate::read_cmd;

/// Compression caps — keep token cost far below "open every matching file"
/// while staying complete enough to be useful.
const MAX_FILES: usize = 40;
const MAX_PER_FILE: usize = 8;
const MAX_LINE_LEN: usize = 200;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub pattern: String,
    /// Symbol definitions exactly named `pattern` (ranked lead).
    pub symbols: Vec<Symbol>,
    pub matches: TextMatches,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TextMatches {
    /// Total matching lines ripgrep produced (before file capping).
    pub count: usize,
    pub files: Vec<FileMatches>,
    /// True when more files matched than were returned (`MAX_FILES`).
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct FileMatches {
    pub path: String,
    pub lines: Vec<LineMatch>,
    /// True when this file had more matches than `MAX_PER_FILE`.
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct LineMatch {
    pub line: u32,
    pub text: String,
}

pub fn run(
    repo_root: &Path,
    pattern: String,
    lang: Option<String>,
    limit: usize,
    render: Render,
    gain_opts: GainOpts,
) -> Result<()> {
    read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let backend = TreeSitterBackend::new(store);

    // Lead: exact-name symbol definitions.
    let q = SymbolQuery {
        query: pattern.clone(),
        kind: None,
        language: lang.clone(),
        limit: 0,
    };
    let mut symbols = backend.workspace_symbols(&q)?;
    symbols.retain(|s| s.name == pattern);
    let sym_cap = if limit == 0 { usize::MAX } else { limit };
    symbols.truncate(sym_cap);

    // Complete: every textual occurrence, via real ripgrep.
    let (matches, rg_ran) = ripgrep(repo_root, &pattern, lang.as_deref(), limit);

    let advisory = advisory(&matches, rg_ran);
    let candidate_paths = candidate_paths(&symbols, &matches);
    let result = SearchResult {
        pattern: pattern.clone(),
        symbols,
        matches,
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

/// Run ripgrep in the repo and parse `path:line:text`. Returns the compressed
/// matches and whether ripgrep actually ran (false = not on PATH → caller
/// advises). Never errors: a missing/var ripgrep degrades to empty matches.
fn ripgrep(
    repo_root: &Path,
    pattern: &str,
    lang: Option<&str>,
    limit: usize,
) -> (TextMatches, bool) {
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
        Err(_) => {
            return (
                TextMatches {
                    count: 0,
                    files: Vec::new(),
                    truncated: false,
                },
                false,
            )
        }
    };
    // rg exit: 0 = matches, 1 = none, 2 = error. Treat non-2 as ran.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut files: Vec<FileMatches> = Vec::new();
    let mut count = 0usize;
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
        count += 1;
        let text = truncate(text.trim_end(), MAX_LINE_LEN);
        // rg groups by file, so the current file is the last entry.
        match files.last_mut() {
            Some(f) if f.path == path => {
                if f.lines.len() < MAX_PER_FILE {
                    f.lines.push(LineMatch { line: lineno, text });
                } else {
                    f.truncated = true;
                }
            }
            _ => {
                if files.len() >= max_files {
                    files_truncated = true;
                    continue;
                }
                files.push(FileMatches {
                    path: path.to_string(),
                    lines: vec![LineMatch { line: lineno, text }],
                    truncated: false,
                });
            }
        }
    }
    (
        TextMatches {
            count,
            files,
            truncated: files_truncated,
        },
        true,
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    t.push('…');
    t
}

fn advisory(m: &TextMatches, rg_ran: bool) -> Option<String> {
    if !rg_ran {
        return Some(
            "ripgrep not on PATH — textual matches unavailable; showing symbol \
             definitions only. Install ripgrep for complete results."
                .into(),
        );
    }
    if m.truncated || m.files.iter().any(|f| f.truncated) {
        return Some(format!(
            "results truncated (caps: {MAX_FILES} files, {MAX_PER_FILE}/file) — \
             narrow the pattern or run `rg` directly for the full set"
        ));
    }
    None
}

fn candidate_paths(symbols: &[Symbol], matches: &TextMatches) -> Vec<String> {
    let mut out: Vec<String> = symbols.iter().map(|s| s.location.path.clone()).collect();
    out.extend(matches.files.iter().map(|f| f.path.clone()));
    out.sort();
    out.dedup();
    out
}

/// repoctx language slug → ripgrep `--type` name (only where they differ or
/// need confirming). None → don't constrain ripgrep by type.
fn rg_type(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "rust" => "rust",
        "go" => "go",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "tsx" => "ts",
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
        if !self.symbols.is_empty() {
            out.push_str("definitions:\n");
            for s in &self.symbols {
                out.push_str(&format!(
                    "  {}:{}  {}  {}\n",
                    s.location.path,
                    s.location.start_line + 1,
                    s.name,
                    s.kind.as_str()
                ));
            }
            out.push('\n');
        }
        if self.matches.files.is_empty() {
            out.push_str(if self.symbols.is_empty() {
                "no matches"
            } else {
                "no other textual matches"
            });
        } else {
            out.push_str(&format!("matches ({}):\n", self.matches.count));
            for f in &self.matches.files {
                for l in &f.lines {
                    out.push_str(&format!("  {}:{}  {}\n", f.path, l.line, l.text));
                }
                if f.truncated {
                    out.push_str(&format!("  {} … (more in file)\n", f.path));
                }
            }
            if self.matches.truncated {
                out.push_str("  … (more files)\n");
            }
        }
        if let Some(a) = &self.advisory {
            out.push_str("\nadvisory: ");
            out.push_str(a);
        }
        out
    }
}
