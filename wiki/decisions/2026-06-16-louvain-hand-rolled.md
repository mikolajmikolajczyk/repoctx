# Communities clustering: hand-rolled Louvain over a plain adjacency, not petgraph

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** library-choice | algorithm

## Context

`repoctx communities` (issue #14) clusters the resolved call graph into
subsystems via Louvain modularity optimization. The repo already adopted
petgraph as the ephemeral graph helper for `import-cycles` / `modules`
(decision 2026-06-16-petgraph-for-graph-algos), so the default expectation was
to reuse it here too. petgraph ships SCC and toposort but **no community
detection** — Louvain has to be supplied either way.

## Decision

Hand-roll single-level Louvain over a plain `Vec<Vec<(usize, f64)>>` adjacency
inside `communities_cmd.rs`. The modularity math is degree bookkeeping
(`sigma_tot`, weighted degree `k`, `2m`) plus a local-moving pass — it wants a
flat numeric representation, not petgraph's node/edge store, and there is no
crate in the tree that provides Louvain. Wrapping petgraph here would add an
adapter layer (build a `Graph`, then read indices back out) for zero algorithmic
benefit.

This is **not** a reversal of the petgraph decision: petgraph stays the choice
for the standard algorithms it actually ships (SCC, toposort). Louvain is simply
outside its surface, so it lives as a self-contained helper next to its only
caller.

## Alternatives considered

- **petgraph + external Louvain crate** — no maintained Louvain crate fit the
  tree cleanly; adds a dependency for one call site.
- **petgraph adjacency, hand-rolled Louvain on top** — petgraph buys nothing
  for the modularity loop; the index round-tripping is pure overhead.
- **Full multilevel Louvain (aggregation phases)** — overkill for an
  orientation command; single-level local-moving already separates clear
  subsystems. Revisit only if clusters prove too coarse on large graphs.

## Trigger to revisit

Single-level clustering produces too-coarse or unstable communities on real
large repos, or a maintained Louvain/Leiden crate lands that fits the tree —
then promote to multilevel or adopt the crate.
