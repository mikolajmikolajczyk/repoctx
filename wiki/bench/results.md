# Benchmark results — repoctx vs ripgrep

Running record of how many tokens `repoctx` saves an agent versus the
naive `ripgrep`-and-read-files loop, on real codebases pinned by SHA.

- **Design + thresholds**: [`wiki/decisions/2026-06-13-agent-bench.md`](../decisions/2026-06-13-agent-bench.md)
- **Harness**: [`scripts/agent-bench/`](../../scripts/agent-bench/) — bats
  suites (pass/fail gates) + `report.sh` (the number table below).
- **Metric**: `bytes/4`, method-consistent with `repoctx gain`. No
  model-specific tokenizer, so numbers are stable + reproducible.
- **Run it**: `scripts/agent-bench/run.sh --clone` (gates), then
  `BENCH_CLONES=/tmp/repoctx-bench scripts/agent-bench/report.sh` (numbers).

## Cost model recap

For one agent intent ("where is X", "show me X", "what's in this file")
we price three ways to answer it:

- **repoctx** — tokens in `repoctx <cmd> --json` stdout.
- **rg-worst** — `rg` match output + `bytes/4` of *every* file the
  pattern hits (the agent opens all candidates to disambiguate).
- **rg-best** — `rg` match output + `bytes/4` of the *single* top-match
  file (agent guesses right first try).

`save vs worst` / `save vs best` are the percentage of tokens repoctx
avoids against each.

---

## v0.7.0 — 2026-06-13

repoctx 0.7.0, metric bytes/4. Pinned SHAs per the design doc.

### helix-editor/helix @ 14eda10 (Rust ~150k LOC)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition Selection` | 43 | 492168 | 78931 | 99% | 99% |
| definition | `definition Editor` | 42 | 442594 | 79073 | 99% | 99% |
| definition | `definition Transaction` | 44 | 265204 | 14600 | 99% | 99% |
| definition | `definition Application` | 43 | 54383 | 4875 | 99% | 99% |
| context | `context render --limit 1 --context 6` | 406 | 103392 | 15864 | 99% | 97% |
| symbols | `symbols Transaction --limit 20` | 376 | 265204 | 14600 | 99% | 97% |
| outline | `outline helix-core/src/selection.rs` | 3525 | 772255 | 48397 | 99% | 92% |

### vuejs/core @ 478e3e8 (TypeScript)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition defineComponent` | 185 | 511505 | 50378 | 99% | 99% |
| definition | `definition reactive` | 84 | 493547 | 57043 | 99% | 99% |
| definition | `definition computed` | 210 | 462410 | 47555 | 99% | 99% |
| definition | `definition effect` | 44 | 447031 | 50597 | 99% | 99% |
| context | `context watch --limit 1 --context 6` | 1547 | 4511 | 2583 | 65% | 40% |
| symbols | `symbols ref --limit 20` | 847 | 1166253 | 77934 | 99% | 98% |
| outline | `outline packages/compiler-sfc/src/parse.ts` | 778 | 389796 | 15334 | 99% | 94% |

### rust-lang/rust-analyzer @ e79b822 (Rust ~500k LOC)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition Semantics` | 43 | 1038619 | 175883 | 99% | 99% |
| definition | `definition SourceFile` | 47 | 705390 | 173364 | 99% | 99% |
| definition | `definition Analysis` | 41 | 570163 | 6045 | 99% | 99% |
| definition | `definition AssistContext` | 46 | 623936 | 17064 | 99% | 99% |
| context | `context completions --limit 1 --context 6` | 256 | 17256 | 8929 | 98% | 97% |
| symbols | `symbols Completions --limit 20` | 940 | 121344 | 9568 | 99% | 90% |
| outline | `outline crates/ide/src/lib.rs` | 5282 | 3003527 | 219776 | 99% | 97% |

### Reading the numbers

- **`definition` is the headline**: a symbol name maps to a one-line
  location record (~40 tokens) instead of every file the name appears in
  — 99% off worst-case across all three repos, and still 99% off even
  the optimistic single-file open.
- **`outline` / `symbols`** stay at 99% vs worst because they replace
  reading whole files / scanning every hit with a structured list.
- **`context` is the deliberate floor.** It *returns the source window*
  the agent asked for, so it can't compress like a pointer does. The one
  sub-50%-vs-best row is `vuejs context watch` (40% vs best): `watch` is
  a large multi-overload function, so repoctx's 6-line window plus all
  overloads (1547 tokens) is close to just opening the one file (2583).
  It still beats worst-case (65%) and clears the suite gate (≥60% vs
  worst). This is the expected shape, not a regression — see the design
  doc's context note.

All bats gates pass at these numbers: helix 8/8, vuejs-core 8/8,
rust-analyzer 9/9.

---

## Template (copy per release)

```markdown
## vX.Y.Z — YYYY-MM-DD

repoctx X.Y.Z, metric bytes/4. Pinned SHAs per the design doc.

### <repo> @ <sha7> (<lang>)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| ... | ... | ... | ... | ... | ...% | ...% |
```

Regenerate the table with `report.sh`; paste under a new `## vX.Y.Z`
heading (newest on top). If a pinned SHA was bumped this release, record
the old→new SHA here per the design doc's reproducibility policy.
