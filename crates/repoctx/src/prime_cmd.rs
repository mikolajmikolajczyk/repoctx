//! `repoctx prime` — session-start orientation digest (issue #11, adoption).
//!
//! A compact, token-budgeted repo map meant to be injected into an agent's
//! context at session start (via a SessionStart hook), so the agent begins with
//! a structural picture and reaches for `repoctx` instead of blind `grep`/`cat`.
//! This is the "prime the agent once" adoption path — the opposite of
//! intercepting every command. Deterministic, no LLM.
//!
//! Budget-conscious by construction: headline plus top subsystems (3 members
//! each), a hub line, entry points, and a one-block skill pointer — on the
//! order of 600 to 900 tokens. It pays for itself the first time the agent
//! skips a repo-wide `cat`/`grep` flail. The full call graph is referenced by
//! command, never inlined.

use std::path::Path;

use anyhow::{Context, Result};
use repoctx_store::Store;

use crate::communities_cmd::{build_communities, resolved_graph, top_god_nodes};
use crate::config::Config;

const MAX_SUBSYSTEMS: usize = 8;
const MEMBERS_PER_SUBSYSTEM: usize = 3;
const MAX_HUBS: usize = 8;
const MAX_ENTRY_POINTS: usize = 8;
const MAX_LANGS: usize = 6;

pub fn run(repo_root: &Path) -> Result<()> {
    // Never cold-index at session start — a full first-time index would add
    // seconds of latency to every fresh session. If there's no index yet, emit
    // a one-line nudge instead; otherwise do the cheap incremental refresh.
    if !repo_root.join(".repoctx/index.db").exists() {
        println!(
            "repoctx is available here but the repo isn't indexed yet. Run \
             `repoctx index` once, then use `repoctx search/outline/callers/\
             overview` (token-cheap, structure-aware) instead of grep/cat."
        );
        return Ok(());
    }
    crate::read_cmd::ensure_fresh(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let min_size = Config::load(&store)?.analysis.subsystem_min_size;

    let counts = store.counts()?;
    let code_symbols: u64 = store
        .symbol_counts_by_file()?
        .into_iter()
        .map(|(_, _, code)| code)
        .sum();

    let located = store.located_edges()?;
    let graph = resolved_graph(&located).graph;
    let comm = graph.louvain();
    let (subsystems, subsystem_total) = build_communities(
        &graph,
        &comm,
        min_size,
        MAX_SUBSYSTEMS,
        MEMBERS_PER_SUBSYSTEM,
    );
    let hubs = top_god_nodes(&graph, MAX_HUBS);

    let mut out = String::new();
    let name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    out.push_str(&format!(
        "# repoctx map — {name} (auto-generated, deterministic)\n\n"
    ));

    // Headline. Sort languages by symbol count desc so the dominant code
    // languages lead (per_language isn't count-ordered).
    let mut per_lang = counts.per_language.clone();
    per_lang.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let langs: Vec<String> = per_lang
        .iter()
        .take(MAX_LANGS)
        .map(|(l, n)| format!("{l} {n}"))
        .collect();
    out.push_str(&format!(
        "{} files · {} code symbols · {}\n",
        counts.files,
        code_symbols,
        langs.join(", ")
    ));

    // Subsystems.
    if !subsystems.is_empty() {
        out.push_str(&format!(
            "\n## Subsystems ({subsystem_total}, Louvain ≥{min_size})\n"
        ));
        for c in &subsystems {
            out.push_str(&format!(
                "- {} ({}) — {}\n",
                c.label,
                c.size,
                c.members.join(", ")
            ));
        }
    }

    // Hubs (one line, token-thrifty).
    if !hubs.is_empty() {
        let line: Vec<String> = hubs
            .iter()
            .map(|h| format!("{}({})", h.name, h.degree))
            .collect();
        out.push_str(&format!(
            "\n## Hubs (highest-degree)\n{}\n",
            line.join(", ")
        ));
    }

    // Entry points.
    let entries: Vec<String> = store
        .entry_points()?
        .into_iter()
        .take(MAX_ENTRY_POINTS)
        .map(|s| s.file_path)
        .collect();
    if !entries.is_empty() {
        out.push_str(&format!("\n## Entry points\n{}\n", entries.join(", ")));
    }

    // Skill pointer — the adoption nudge. An intent→command cheat-sheet so the
    // agent has a complete path from "I need to navigate" to the exact command,
    // never reaching for grep/cat/find.
    out.push_str(
        "\n## Navigate with repoctx — NOT grep/cat/find (token-cheap, structure-aware)\n\
         For any structural question, run the repoctx command instead of grep/rg/find/bulk-Read:\n\
         - Find a symbol or text → `repoctx search <query>` (exact defs + compressed ripgrep)\n\
         - A file's structure → `repoctx outline <file>` (use a real path — don't guess one)\n\
         - Where defined / show source → `repoctx definition <sym>` · `repoctx context <sym>` (by name, no path needed)\n\
         - Who calls / what it calls → `repoctx callers <sym>` · `callees <sym>` · `callgraph <sym> --direction up|down|both`\n\
         - Blast radius / dead code / cycles → `repoctx impact <sym>` · `deadcode` · `cycles`\n\
         - Imports / layering → `repoctx deps <file>` · `rdeps <module>` · `boundary --from <p> --to <m>`\n\
         - Architecture / orientation → `repoctx overview` · `report` · `communities` · `export --out graph.html`\n\
         - Review a change → `repoctx changed --since <ref>` (changed symbols + their callers)\n\
         - Drill into a subsystem above → `repoctx callgraph <label> --direction both`, or `outline` its files\n\
         Don't know a file's path? Don't guess it — `definition`/`search` return the real path; \
         `context <sym>` shows a symbol's source with no path at all.\n\
         All accept `--json`. Name-based, resolution-aware (ADR-0010). Only fall back to grep/Read when a \
         language is `partial` coverage (check the `advisory`) or you need prose-level reasoning after locating code.\n\
         Full reference: the `repoctx` skill (`.claude/skills/repoctx/SKILL.md`) — or `repoctx <cmd> --help`.\n",
    );

    print!("{out}");
    Ok(())
}
