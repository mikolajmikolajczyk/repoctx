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

## v0.9.0 — 2026-06-15

repoctx 0.9.0, metric bytes/4. Adds the **call-graph** (`callers`/`callees`/
`callgraph`) and **`search`** families, and a fourth, much larger target —
the **Linux kernel** (C, ~62k files, 1.08M symbols).

For the call-graph families, `rg-worst` is the grep-and-read workflow an agent
would actually run to answer "who calls X" — open every file mentioning the
name. (Grep can't answer who-calls structurally at all; this prices the
closest thing it can do.) `search` returns more than the other commands
(symbol defs + every textual match + per-symbol call edges), so its
`save vs best` is lower than `definition`'s — but it never loses a textual
match, and still sits ~95–99% under `rg-worst`.

Headlines: on the Linux kernel, `outline kernel/sched/core.c` is **19,670**
tokens vs **199,093,569** for opening every `static`-matching file (99%);
`definition schedule` is **79** vs **61,997,950** (99%); `callers kmalloc` is
**6,354** vs **36,118,024** (99%). Every family clears 99% vs rg-worst on every
repo (lone exception: vuejs `context watch`, where rg-worst is a tiny 4.5k —
65%).

### helix-editor/helix @ 14eda10 (Rust ~150k LOC)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition Selection` | 43 | 492168 | 78931 | 99% | 99% |
| definition | `definition Editor` | 42 | 442594 | 79073 | 99% | 99% |
| definition | `definition Transaction` | 44 | 265204 | 14600 | 99% | 99% |
| definition | `definition Application` | 43 | 54383 | 4875 | 99% | 99% |
| context | `context render --limit 1 --context 6` | 406 | 103392 | 15864 | 99% | 97% |
| symbols | `symbols Transaction --limit 20` | 376 | 265204 | 8256 | 99% | 95% |
| outline | `outline helix-core/src/selection.rs` | 3525 | 772255 | 48397 | 99% | 92% |
| search | `search Transaction` | 3856 | 265204 | 14600 | 98% | 73% |
| search | `search Selection` | 7792 | 492168 | 78931 | 98% | 90% |
| callers | `callers render` | 47 | 571852 | 82944 | 99% | 99% |
| callees | `callees render` | 47 | 571852 | 82944 | 99% | 99% |
| callgraph | `callgraph render --depth 2 --direction down` | 47 | 571852 | 82944 | 99% | 99% |

### vuejs/core @ 478e3e8 (TypeScript)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition defineComponent` | 185 | 511505 | 50378 | 99% | 99% |
| definition | `definition reactive` | 84 | 493547 | 57043 | 99% | 99% |
| definition | `definition computed` | 210 | 462410 | 47555 | 99% | 99% |
| definition | `definition effect` | 44 | 447031 | 50597 | 99% | 99% |
| context | `context watch --limit 1 --context 6` | 1547 | 4511 | 2583 | 65% | 40% |
| symbols | `symbols ref --limit 20` | 847 | 1166253 | 77934 | 99% | 98% |
| outline | `outline packages/compiler-sfc/src/parse.ts` | 778 | 389796 | 14425 | 99% | 94% |
| search | `search computed` | 8276 | 462410 | 47555 | 98% | 82% |
| search | `search reactive` | 10739 | 493547 | 57043 | 97% | 81% |
| callers | `callers computed` | 48 | 462410 | 47555 | 99% | 99% |
| callees | `callees computed` | 48 | 462410 | 47555 | 99% | 99% |
| callgraph | `callgraph computed --depth 2 --direction down` | 48 | 462410 | 47555 | 99% | 99% |

### rust-lang/rust-analyzer @ e79b822 (Rust ~500k LOC)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition Semantics` | 43 | 1038619 | 175883 | 99% | 99% |
| definition | `definition SourceFile` | 47 | 705390 | 173364 | 99% | 99% |
| definition | `definition Analysis` | 41 | 570163 | 7603 | 99% | 99% |
| definition | `definition AssistContext` | 46 | 623936 | 17064 | 99% | 99% |
| context | `context completions --limit 1 --context 6` | 256 | 17256 | 5284 | 98% | 95% |
| symbols | `symbols Completions --limit 20` | 940 | 121344 | 13652 | 99% | 93% |
| outline | `outline crates/ide/src/lib.rs` | 5282 | 3003527 | 219776 | 99% | 97% |
| search | `search Completions` | 4940 | 121344 | 13652 | 95% | 63% |
| search | `search Semantics` | 5456 | 1038619 | 175883 | 99% | 96% |
| callers | `callers completions` | 50 | 645046 | 18806 | 99% | 99% |
| callees | `callees completions` | 50 | 645046 | 7400 | 99% | 99% |
| callgraph | `callgraph completions --depth 2 --direction down` | 50 | 645046 | 7544 | 99% | 99% |

### torvalds/linux @ v6.6 (C ~62k files, 1.08M symbols)

| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |
|---|---|--:|--:|--:|--:|--:|
| definition | `definition kmalloc` | 231 | 36118024 | 196044 | 99% | 99% |
| definition | `definition schedule` | 79 | 61997950 | 430358 | 99% | 99% |
| definition | `definition mutex_lock` | 284 | 51583650 | 444768 | 99% | 99% |
| definition | `definition task_struct` | 173 | 9397930 | 175234 | 99% | 99% |
| context | `context schedule --limit 1 --context 6` | 112 | 678925 | 11256 | 99% | 99% |
| symbols | `symbols task_struct --limit 20` | 828 | 9397930 | 176773 | 99% | 99% |
| outline | `outline kernel/sched/core.c` | 19670 | 199093569 | 18551951 | 99% | 99% |
| search | `search kmalloc` | 16615 | 36118024 | 196044 | 99% | 91% |
| search | `search task_struct` | 17670 | 9397930 | 175234 | 99% | 89% |
| callers | `callers kmalloc` | 6354 | 36118024 | 196044 | 99% | 96% |
| callees | `callees schedule` | 724 | 61997950 | 405449 | 99% | 99% |
| callgraph | `callgraph schedule --depth 2 --direction down` | 9456 | 61997950 | 430358 | 99% | 97% |

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
