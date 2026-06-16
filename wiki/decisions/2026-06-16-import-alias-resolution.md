# Import alias resolution — tsconfig paths, query-time, repo-root scan

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** import-graph | resolution

## Context

The import graph (ADR-0011) stored raw specifiers and resolved only relative
(`./`/`../`) imports. On a real TS repo (madside) that left **393 of ~680
edges external** — every `@adapters/*`-style alias — which made `modules`
incomplete and, worse, made `boundary` report a false "clean" (#13): it
couldn't see aliased layer crossings at all. Issue #8.

## Decision

Resolve **tsconfig path aliases** in a shared `ImportResolver`
(`crates/repoctx/src/resolver.rs`), used by `modules`, `import-cycles`, and
`boundary`. Specifics:

- **Query-time**, not index-time. The resolver is built per-command from one
  store query (`all_file_paths`) + the tsconfig(s) read off disk, then dropped
  — same ephemeral posture as the petgraph helper. SQLite stays source of
  truth; no resolved edges are persisted (a future LSP/semantic pass can write
  `resolution='semantic'` rows if precise resolution is ever needed).
- **Alias source = repo-root scan.** Collect `compilerOptions.paths`/`baseUrl`
  from *every* `tsconfig*.json` / `jsconfig.json` at the repo root and merge.
  This covers split `tsconfig.base.json` + `tsconfig.app.json` and `extends`
  chains **without** resolving the chain — pragmatic and robust.
- **JSONC-tolerant** parsing (strip comments + trailing commas, quote-aware).
- Scope: **TS/JS only**. Bare/package specifiers (`react`) and non-TS module
  syntax (Rust `mod`/`use`, Python packages, Go module paths) stay external —
  future per-language work under #8.

## Alternatives considered

- **Index-time resolution** (store resolved file→file edges) — rejected: the
  resolution depends on config that can change independently of source; keeping
  it query-time avoids stale edges and a schema change.
- **Chase `extends` chains** — rejected as unnecessary: scanning all root
  tsconfigs catches the same aliases with less code and no path-resolution edge
  cases.

## Consequences

- madside: `modules` external edges 393 → 163; `boundary` sees alias crossings;
  `boundary count: 0` now reports residual unresolved (bare) imports instead of
  a misleading clean. Closes the #13 false-confidence gap for real.
- The `ImportResolver` is the natural home for future per-language resolvers.
