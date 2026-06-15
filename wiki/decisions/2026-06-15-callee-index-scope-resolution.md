# Callee resolution by index scope (internal / external)

**Date**: 2026-06-15. **Issue**: `cd2680f`. Refines [search provenance](2026-06-15-search-provenance.md).

## Context

`search` callees were dominated by noise: `format`/`Some`/`Ok`/`get`/`push`
(stdlib/builtins) shown as `unresolved`, and ambiguous names (e.g. four
unrelated `new`) expanded to four full wrong-guess locations. The one valuable
fact — "what of *your* code does this call" — was buried.

## Root cause

`unresolved` conflated two facts: (1) callee is external (stdlib/3rd-party —
expected, the bulk of the noise) and (2) callee should resolve but didn't
(rare). Both read as "failure."

## Decision

Group call edges by **how the name resolves within the indexed scope**:

- **`internal`** — exactly one indexed symbol. Expanded with location.
- **`ambiguous`** — several indexed symbols. Collapsed to a per-name count
  (`{name, count}`); candidates only when expanded.
- **`external`** — no definition in the indexed scope. Collapsed to a count;
  names only when expanded.

Default output expands `internal`, collapses `ambiguous`/`external` to counts.
`--all-callees` expands them. Same grouping on `callers` (in practice always
internal). No new data — this reinterprets the resolver's existing
in-index/out-of-index fact, which we were mislabeling as `unresolved`.

## `external` = "not in the indexed scope", NOT "outside the repo"

Defined by **what we parsed**, not the repo boundary. An uncovered-language
file's symbols aren't indexed, so a call into them is `external` even though
"in the repo" — and that label is *truthful* (we genuinely didn't parse it).
Workspace/multi-repo indexing later expands "the indexed scope" and the labels
keep meaning the right thing, no change. Callee resolution has no textual
fallback, so a misclassification is silent and not self-correcting — the
scope-based definition is what keeps it honest.

## Non-goals

- No stop-list of builtin names. External-ness = index absence only.
- No `external` → `internal` promotion by guessing.
- No cross-language / FFI resolution.

## Trigger to revisit

If `internal-ambiguous` counts are frequently the real answer, consider a
type-aware resolver (the LSP path) rather than expanding candidates by default.
