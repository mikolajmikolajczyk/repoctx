# ADR-0011 — Static, string-based import / dependency graph (Tree-sitter)

- **Status**: Accepted
- **Date**: 2026-06-15
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: backend, indexing, import-graph, schema, languages

## Context

After the call graph (ADR-0010) shipped, the next-highest-value structural
relationship `repoctx` lacks is the **import / dependency graph**: what a file
imports (`deps`), what imports a module (`rdeps`), and the architectural
questions that build on it — boundary/layering checks ("does `@ui` import
`@adapters`?"), import cycles, module dependency maps, public API surface.

These are exactly the queries agents currently fake with `rg` over import
lines and eslint-boundary comments — textual, lossy, and blind to structure.
Tree-sitter already parses import statements, so the edges are extractable
with the same machinery the call graph uses.

## Decision

Ship a **static, string-based** import graph now (epic #4), mirroring
ADR-0010 wholesale:

- A schema-v5 `imports` table: one row per import SITE — `(file_path,
  module, site_line, site_column, resolution)`. `module` is the raw
  specifier as written in source (quotes/angle-brackets stripped):
  `@adapters/storage-idb`, `./util`, `std::collections::HashMap`, `os`,
  `stdio.h`.
- Per-language Tree-sitter import queries (`@module` capture) for the core
  8 languages (Rust, Python, JavaScript, TypeScript, TSX, Go, C, C++, Java);
  no-op elsewhere until a follow-up adds them.
- Edges cascade with the file via the `file_path` FK, so incremental
  reindex replaces a file's import edges atomically (same as `calls`).
- `deps <file>` queries by `file_path`; `rdeps <module>` matches any
  specifier *containing* the argument as a substring (so `rdeps storage-idb`
  finds importers of `@adapters/storage-idb`).

## Decision drivers

- Match `repoctx`'s accuracy class: syntax-derived, string-based, advisory
  on empties. No precise specifier→file resolution yet.
- Reuse the call-graph design (extract → store → query, `resolution`
  column) rather than invent a second mechanism.
- Stay a single static binary — no resolver toolchains (tsconfig paths,
  node_modules walks, crate layout) in this slice.

## Considered options

1. **Static string-based now; a resolver enriches the same table later.**
   (chosen)
2. **Resolve specifiers to files at index time** (tsconfig/node_modules/crate
   resolution) — rejected for this slice: language-specific, toolchain-heavy,
   and not needed for boundary checks that operate on alias prefixes.
3. **Store imports as `calls`-style symbol edges** — rejected: imports point
   at modules/specifiers, not symbols; a separate table is clearer.

## Consequences

- `deps`/`rdeps` answer dependency questions structurally; boundary checks,
  cycle detection, module maps, and public-API surface (epic #4 children)
  build on the same table.
- Precise specifier→file resolution is deferred. A future resolver (or LSP
  backend) writes `resolution = 'semantic'` rows into the same table — no
  schema fork, exactly as ADR-0010 plans for semantic call edges.
- `rdeps` substring matching can over-match (e.g. `util` matches
  `./my-util`); the advisory + `--json` exact `module` field let callers
  disambiguate. Acceptable for an advisory-class tool.
- Relative specifiers (`./x`, `../y`) are stored verbatim, so `rdeps` by
  bare module name is most useful for aliased / package specifiers; `deps`
  by file is exact regardless.
