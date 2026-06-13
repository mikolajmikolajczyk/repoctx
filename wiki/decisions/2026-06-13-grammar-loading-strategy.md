# Grammar loading strategy — static linking

**Date**: 2026-06-13. **Issue**: `0206373`. **Epic**: `9cf4c18`. **Milestone**: v0.7.0.

## What

Decide how repoctx loads Tree-sitter grammars before adding the v0.7.0
language batch (ruby, c, cpp, bash, java, kotlin, swift, c-sharp, php,
lua). Gates every grammar child.

## Decision: keep static linking

Grammars stay statically compiled into the `repoctx` binary via their
`tree-sitter-<lang>` crates, exactly as the initial set (ADR-0002). Each
new language is an enum variant + a registry arm + a `tags.scm` (the
crate's `TAGS_QUERY` when it ships one, else a vendored Aider query under
`crates/index/queries/`). No runtime grammar loading.

## Alternatives considered

- **Dynamic loading** (`dlopen` compiled `.so`/`.dylib`/`.dll` grammars
  at runtime) — rejected. Adds an ABI-versioning surface against the
  Tree-sitter core, a per-platform packaging story (ship N shared libs ×
  3 OSes), a filesystem-discovery + trust problem, and unsafe FFI. Kills
  the "single self-contained binary, works offline" property that the
  v0.5.3 fetcher deletion just doubled down on.
- **Hybrid** (core set static, long tail dynamic) — rejected. Worst of
  both: still needs the dynamic machinery, plus a split mental model for
  which languages live where.

## Consequences / costs

- **Binary size grows ~linearly** per grammar (each compiled parser is
  roughly 0.3–2 MB). The v0.7.0 batch is expected to add a few MB. Still
  well under any distribution concern; revisit only if it approaches
  ~50 MB. `cargo build --release` size delta is recorded per grammar PR.
- **Compile-time + dependency surface grows.** Each grammar crate must be
  compatible with the pinned Tree-sitter core (0.25.x). Crates that lag
  (still on core ^0.20/^0.23) or pull a conflicting core via a non-default
  feature are NOT added until a compatible release exists — documented in
  `wiki/decisions/2026-06-11-grammar-crate-selection.md` alongside the
  existing `toml-ng` / `md` pins. A grammar that can't be made compatible
  is deferred, not forced.

## Per-grammar checklist (for the child issues)

1. Add `tree-sitter-<lang>` to `[workspace.dependencies]` + `crates/index`,
   pinned to a core-0.25-compatible version.
2. `Language` enum: variant + `slug` / `from_slug` / `from_extension` /
   `ts_language` / `tags_query` / `coverage` / `notes`, and add to
   `ALL_LANGUAGES`.
3. `extractor::compiled_for`: a `slot!` arm.
4. tags query: use the crate's `TAGS_QUERY` if it exists and uses the
   `@definition.<kind>` capture form; otherwise vendor Aider's
   `<lang>-tags.scm` (Apache-2.0) under `crates/index/queries/` and
   `include_str!` it.
5. A fixture + extraction test (known symbol inventory) — same shape as
   the accuracy-parity suite.
6. `repoctx languages` + the coverage matrix pick the new language up
   automatically via `ALL_LANGUAGES`.

## Trigger to revisit

Binary size approaching ~50 MB, or a genuine user need for grammars we
can't vendor (e.g. proprietary / fast-moving) — then reconsider dynamic
loading for the long tail only.
