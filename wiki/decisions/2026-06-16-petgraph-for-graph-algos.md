# Use petgraph for graph algorithms in #4/#5/#6 — as an ephemeral helper, not the model

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** library-choice | call-graph | import-graph

## Context

The call graph (ADR-0010) and import graph (ADR-0011) ship as **name-based
edges in SQLite**, resolved at query time. Narrow queries (`callers`,
`callees`, `deps`, `rdeps`, `boundary`) are plain SQL — no graph library
needed. But the remaining graph-algorithm work hand-rolls traversal:
`callgraph` (manual BFS) and `cycles` (manual iterative color-DFS + canonical
dedup). The upcoming work in #4 (import-cycle detection, module dependency
map), #5 (overview hotspots/centrality), and #6 (changed → reverse
reachability) is more algorithm-heavy still.

## Decision

When implementing the graph-algorithm parts of **#4, #5, #6**, use
[`petgraph`](https://github.com/petgraph/petgraph) (`tarjan_scc`, `toposort`,
`is_cyclic`, BFS/DFS, centrality) **instead of hand-rolling**. Don't rewrite
the already-shipped `callgraph`/`cycles` reactively, and never put a graph lib
behind the SQL queries that don't need one.

**Strict boundary — petgraph is an ephemeral compute structure, never the
storage model:**

- Build the name-graph from a single store query (the `resolved_edge_pairs`
  pattern) → run the algorithm → drop it. Per-command, not persisted.
- SQLite stays the source of truth (ADR-0003); name resolution stays at query
  time; incremental indexing is untouched.
- Keep the existing edge caps (e.g. `cycles` skips >20k edges) so a cold-start
  CLI never blows up on Linux-scale repos.

## Alternatives considered

- **Keep hand-rolling** — lost: SCC/toposort/centrality are exactly the fiddly,
  bug-prone code petgraph does correctly; the algorithm surface is about to
  quadruple (4 more commands).
- **Make petgraph the in-memory graph model** (load whole graph, query it) —
  rejected: breaks the SQLite-source-of-truth + narrow-query + cold-start +
  Linux-scale architecture. This is the failure mode to avoid.

## Trigger to revisit

If a daemon (ADR-0005) ever holds a warm graph in memory, reconsider whether a
persisted petgraph model earns its keep. Until then: ephemeral only.
