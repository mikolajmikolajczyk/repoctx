# ADR-0002 — Tree-sitter as the primary indexing backend

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: backend, parsing, indexing, languages

## Context

`repoctx` needs to extract symbols, file outlines, and structural information across many languages, with no language toolchains installed and no daemon running. The MVP commands (`index`, `symbols`, `definition`, `outline`, `context`, `status`) are all answerable from syntax alone — semantic queries (`references`, `hover`, `callers`) are deferred to the LSP path (ADR-0005).

## Decision drivers

- Multi-language coverage out of the box, single static binary.
- No per-language toolchain required for basic queries.
- Incremental parsing for fast re-indexing on edits.
- Mature Rust bindings (`tree-sitter` crate).
- Existing grammars ship reusable extraction queries (`tags.scm`, `locals.scm`) — we should reuse, not reinvent.

## Considered options

1. **Tree-sitter** as primary; LSP optional for semantic queries (ADR-0005).
2. **LSP-first**: rely on language servers for everything.
3. **Hand-rolled parsers per language**.
4. **ctags / universal-ctags** as primary.

## Decision outcome

**Tree-sitter is the primary indexing backend.** Grammars are statically linked into the `repoctx` binary — no plugin system in MVP. Symbol extraction reuses each grammar's upstream `tags.scm` (and `locals.scm` where helpful); custom queries are introduced only when an upstream query is missing or actively wrong.

Two consequences of "reuse upstream", accepted deliberately:

- **Kind vocabulary is upstream's, as-is.** Upstream tags queries collapse kinds (Rust struct/enum/union/type emit `@definition.class`; Go struct/interface emit `@definition.type`). We map captures to our `SymbolKind` without re-tagging — Rust `struct` indexes as kind `class`. Amending upstream queries per-language is the escape hatch if this ever hurts in practice.
- **Data/doc languages get minimal custom queries** (the "missing upstream query" case): Markdown → headings (kind `section`), JSON/YAML/TOML → top-level keys (kind `key`). Vendored under `crates/index/queries/` with provenance comments.

Concrete crate selection and version pins live in the decision log (`wiki/decisions/2026-06-11-grammar-crate-selection.md`) — they churn faster than this ADR.

### Initial language set (MVP)

- Go
- Rust
- TypeScript
- JavaScript
- Python
- JSON
- YAML
- TOML
- Markdown

Adding a language = adding a grammar dep + the upstream tag query + a small mapping into our `Symbol`/`SymbolKind` types. No plugin loading, no dynamic discovery.

## Positive consequences

- Broad language coverage with one static binary.
- No dependency on per-language toolchains for the default install.
- Incremental parsing aligns with mtime-based cache invalidation (ADR-0006) and file-local updates (ADR-0007).
- Reusing upstream `tags.scm` keeps our maintenance surface small and tracks community improvements.

## Negative consequences

- Tree-sitter gives syntax, not semantics: cross-file resolution (`references`, type-aware `definition`, call graphs) is limited or impossible from Tree-sitter alone — handled by ADR-0005.
- Per-language coverage depends on upstream `tags.scm` quality; gaps require us to ship overrides.
- Statically linking many grammars grows binary size. Accepted for the agent UX win.

## Links

- ADR-0004 (backend abstraction) — defines how Tree-sitter and LSP coexist behind one API.
- ADR-0005 (LSP via `repoctxd`) — the semantic-enrichment path.
- ADR-0007 (incremental file-local updates) — consumes Tree-sitter parses.
- ADR-0010 (static call graph) — un-defers `callers` via name-based syntax edges; the "`callers` deferred to the LSP path" note above is superseded for the static (`'syntactic'`) case.
