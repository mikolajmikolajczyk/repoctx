# Graph node identity = (name, file, line); resolved-only degree; host-method exclusion

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** algorithm | correctness

## Context

`communities`, `report`, god-node ranking, and `export` all build an in-memory
graph from the call graph. The first cut keyed graph nodes by bare symbol
**name**. On any codebase with common method names (most JS/TS), distinct
definitions sharing a name (`set`, `join`, `create`) collapsed into one node
whose degree summed across unrelated definitions — fabricating fake super-hubs
and meaningless grab-bag clusters. Separately, receiver-blind method calls
(`x.set()`) bind to a lone same-named function under name-based resolution
(ADR-0010), inflating that function as popularity rather than centrality.

## Decision

1. **Node identity = `(name, file, line)`** (a definition's location), not bare
   name. Same-named definitions stay distinct nodes. Display labels stay bare
   for unique names and qualify (`name@file:line`) only when a name has >1
   definition. Built via `store::located_edges()` →
   `communities_cmd::resolved_graph()`.
2. **Degree + clustering use resolved (unambiguous) edges only** — extends the
   ADR-0010 resolved-only clustering rule to god-node degree. Ambiguous callees
   (N candidate defs) are excluded from the resolved graph; `export` still
   *shows* them as dashed edges to one name-bucket node.
3. **Host/builtin method names excluded** from degree/clustering on both
   endpoints, via a single shared `HOST_METHOD_NAMES` SQL list reused by
   `hotspots` and `located_edges`. Heuristic; the real fix is receiver-awareness
   (#9).

## Alternatives considered

- **Distribute ambiguous edge weight 1/N across candidates** — more complex,
  rarely changes the orientation-level picture. Rejected.
- **Keep name-based nodes, only stop-list method names** — leaves multi-def
  collapse for non-method names; doesn't fix clustering. Rejected.
- **Receiver-aware resolution now** — the correct long-term fix (#9), but a
  much larger change to extraction; the stop-list is the pragmatic bridge.

## Trigger to revisit

Receiver-awareness (#9) lands → drop the host-method stop-list. Or a real LSP
backend provides semantic resolution → node identity comes from symbol ids
instead of `(name, file, line)`.
