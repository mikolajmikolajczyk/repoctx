# repoctx expansion roadmap — beyond grep replacement

Staging doc for future work, filed as GitHub issues. Captures the
"repoctx has more potential than ripgrep/grep" direction.

## Framing

grep matches **text**; repoctx is an index of code **structure**. Every
question about a structural *relationship* is repoctx territory — grep can
only fake it by reading everything. The call graph (v0.8.0) was the first
relationship. This roadmap is the ladder above it.

Through-line for docs/skill/marketing: *repoctx answers how code relates —
definitions, calls, imports, impact, dead code, architecture — not just where
text appears.*

## Tier 1 — near-free (new queries over the existing `calls` table)

The call-edge data is already there; these are query shapes, not new indexing.

- **Dead code** — symbols with zero incoming call edges (+ not an entry
  point). Grep cannot do this.
- **Impact radius** — transitive *callers* of X = "if I change X, what breaks."
  Already expressible via `callgraph --direction up`; frame it as blast-radius.
- **Call cycles** — cycle detection over the call graph.

→ Candidate issue: `feat: dead-code + impact-radius queries over the call graph`.

## Tier 2 — next epic: import / dependency graph (highest leverage)

Same machinery as the call graph (extract edges → store → query, name-based,
`resolution` column, LSP-ready), but over **import statements** (tree-sitter
sees them).

- `repoctx deps <file>` / `rdeps <file>` — what it imports / what imports it.
- **Boundary / layering checks** — "does `@plugins` import `@adapters`?"
  answered from the import graph, **no eslint needed**. (Directly serves the
  real boundary-leak hunts — regex-over-comments today, structural instead.)
- **Import cycles**, **module dependency map**, **public API surface**
  (exported symbols per module).

→ Candidate epic: `epic: import/dependency graph`.
This is the clearest "way more than grep" story and reuses the call-graph
design wholesale.

## Tier 3 — synthesis: onboarding / overview

- **`repoctx overview`** — one call → architecture map: modules, sizes, entry
  points, public surface, hotspots (most-called symbols). The "agent dropped
  into an unfamiliar repo" case; replaces dozens of `ls`/`cat`/grep round-trips.

→ Candidate issue: `feat: repoctx overview (repo architecture in one call)`.

## Tier 4 — change-aware (review workflows)

- **`repoctx changed [--since REF]`** — git diff → which *symbols* changed →
  their callers = "what this PR touches + its blast radius." Pairs with code
  review.

→ Candidate issue: `feat: changed-symbols + blast radius (git-diff aware)`.

## Tier 5 — semantic (already deferred — LSP path, ADR-0005)

- `references`, `hover`, type-aware `definition`. Needs warm LSP servers; lives
  behind the `repoctxd` daemon epic (GitHub issues). Out of scope until then.

## Recommended order

1. **Tier 1 quick win** — dead-code + impact: almost free on the existing
   `calls` table; immediately demonstrates "repoctx does things grep can't."
2. **Tier 2 epic** — import/dependency graph: reuses call-graph machinery,
   answers the architectural questions that keep coming up.
3. Tier 3 (overview) and Tier 4 (changed) after.

(Also still open: call-graph remaining-language extraction — GitHub issue
**#1**. The Tier 1 dead-code + impact-radius work is GitHub issue **#2**.)
