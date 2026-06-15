# search provenance + call-edge surfacing

**Date**: 2026-06-15. **Issue**: `52a1e2c`.

## Context

`repoctx search` returned `{symbols, matches}` — confirmed symbols and raw
textual hits in two separate shapes, but a textual hit could be the real
symbol, a call site, a same-substring symbol, or an incidental string, and
the consumer couldn't tell which. That dilutes repoctx's whole edge: "we
tell you what we know for sure." We also already had call edges
(`callers_of`/`callees_of`) but `search` didn't surface them.

## Decision

One flat, provenance-tagged `results` stream. Each item has `source`:

- **`structural`** — the match line is a tree-sitter symbol definition (name
  may match the query *exactly or by substring* — e.g. `to_call_edges` for
  `call_edges`). We know kind + range. Highest confidence.
- **`reference`** — the line is a call site of the queried name (from the
  call-graph `caller` edges' site locations). Medium.
- **`textual`** — substring matched, AST didn't confirm (comment, string, or
  a call to a *different* symbol). Grep-level.

Each structural item carries its own `callers`/`callees`, queried by that
symbol's name (memoized) — every structural result gets its neighborhood, not
just the exact-query match. (Resolution grouping for those edges is refined in
[the index-scope decision](2026-06-15-callee-index-scope-resolution.md).)

Lines are 0-based in machine output (the rest of the tool's contract);
ripgrep's 1-based numbers are normalized on ingest.

## Deviation from the issue's example

The issue's worked example tagged a same-substring **test function** as
`textual`. We tag it `structural` instead — it *is* a tree-sitter symbol, and
the issue's own definition says "tree-sitter parsed this as a named symbol →
structural." Coherent + honest beats the one inconsistent example; the agent
reads the name to see it isn't the exact target. Every structural symbol
carries its own call edges, so a substring match like `to_call_edges` is just
as navigable as the exact one.

## Guardrails (from the issue)

- No heuristic promotes `textual` → `structural`; only tree-sitter confirms.
- Call edges are AST-derived, name-resolved within the indexed scope only.
- The textual fallback stays — it's labelled, not deleted.

## Trigger to revisit

If the flat stream's heterogeneous rows cost too much in TOON, reconsider the
grouped layout (Option B) the issue floated.
