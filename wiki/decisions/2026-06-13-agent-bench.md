# Agent benchmark — design

**Date**: 2026-06-13. **Issue**: `7296299`. **Epic**: `b20a3c9`.

## What

Pin the benchmark design before the harness lands: query taxonomy,
target repos (with SHAs), threshold bands, walltime budgets, output
format, reproducibility. Subsequent children (`7441dfa` harness,
`866f929`/`255bac3`/`22b09a3` suites, `667e75f` results) implement
against this.

## Cost model

For each query we compare two ways to answer the same agent intent:

- **repoctx**: the tokens in `repoctx <cmd> --json` stdout.
- **ripgrep (worst)**: `rg` match output + `bytes/4` of *every* file the
  pattern hits — the cost if the agent opens every candidate to
  disambiguate (no symbol/kind info to narrow down).
- **ripgrep (best)**: `rg` match output + `bytes/4` of the *single*
  top-match file — the optimistic case where the agent guesses right
  first try.

Tokens are counted at **4 bytes/token** on both sides — method-consistent
with `repoctx gain` (issue `3a7fbc1`). The harness's `tokens` helper
(`7441dfa`) may offer a precise-BPE mode for published headline numbers,
but thresholds here are defined in the bytes/4 metric so they're stable
and reproducible without a model-specific tokenizer.

Real anchor (helix @ pinned SHA, this design's measurement):
`repoctx definition Selection` = 172 bytes (~43 tokens) vs `rg Selection`
hitting **59 files**. Even the best case (read one ~5–20 KB source file =
1.2–5k tokens) is a >90% saving; worst case is >99%.

## Query taxonomy + weighting

Per repo, queries are drawn from four command families, weighted by how
central each is to the agent navigation loop:

| Family | Weight | What it proves |
|---|---|---|
| `definition <name>` | 40% | the flagship "where is X defined" — biggest rg blowup (many candidate files) |
| `context <name>` | 25% | "show me X with surrounding code" — replaces rg + open-file |
| `symbols <query>` | 20% | exploratory substring search |
| `outline <file>` | 15% | "what's in this file" — replaces reading the whole file |

Each suite picks ≥ 3 queries per family from real, recognizable symbols
in that repo (e.g. `Selection`, `Editor`, `Transaction` for helix).

## Target repos (pinned)

| Repo | Lang | SHA | Issue |
|---|---|---|---|
| helix-editor/helix | Rust ~150k LOC | `14eda106f0a3e6a5fc6fb5cbd96bda9774f64ae1` | `255bac3` |
| rust-lang/rust-analyzer | Rust ~500k LOC | `e79b8223f7e0f000d75e7bf9a8f5b590ff7eb7f8` | `22b09a3` |
| vuejs/core | TypeScript | `478e3e83acd34dd213a860be4a2a2bf2090dc26b` | `866f929` |

helix = mid-size Rust baseline; rust-analyzer = large-repo stress; vuejs/core
exercises the vendored TS/TSX `tags.scm`. Shallow-clone at the SHA so runs
are reproducible.

## Threshold bands (pass/fail)

Lower bounds — a run **fails** if it drops below. Conservative vs the
observed >90% so normal variation doesn't flap; tighten once the harness
reports real per-repo aggregates.

| Metric | Threshold |
|---|---|
| Per-query savings vs rg-worst | ≥ 80% |
| Per-query savings vs rg-best (top file only) | ≥ 50% |
| Suite aggregate savings vs rg-worst | ≥ 90% |
| Advisory firing rate on partial-coverage zero-/sparse-hit queries | 100% |

The advisory check is a **correctness** gate, not a savings one: for
JSON/YAML/TOML/Bash queries where coverage is partial, the machine output
must carry the `advisory` field (so the agent knows to fall back to `rg`).
A missing advisory fails the run even if savings look fine.

## Walltime budgets

Excludes the one-time `git clone`. Index build + all queries:

| Scope | Budget |
|---|---|
| helix | ≤ 30 s |
| vuejs/core | ≤ 30 s |
| rust-analyzer | ≤ 90 s |
| whole suite | ≤ 5 min |

(helix indexed in ~0.2 s here; budgets are generous for CI-less laptops.)

## Output format

Both, from one run:

- **Markdown table** to stdout — per-query rows (repoctx / rg-worst /
  rg-best / savings%) + a per-repo + aggregate summary. For PRs + the
  results page (`667e75f`).
- **`--json`** — the same data as structured records for downstream
  charts + version-vs-version drift tracking.

## Reproducibility policy

- Repos pinned by SHA (above). Bump on a deliberate cadence (each minor
  repoctx release, or quarterly), recording the old→new SHA in the
  results page.
- Every result records: repoctx version, repo SHA, date, the bytes/4
  metric. Drift = compare JSON across runs.
- **Manual / scheduled only — never per-PR CI.** Clones are large, runs
  hit many files; this is a "get a number" tool, not a gate. A future
  scheduled workflow (weekly) may post to the results page.

## Acceptance

- [x] decision doc landed
- [x] target repo list pinned with SHAs
- [x] threshold table populated with concrete numbers (no TODOs)
