# Adoption-probe results

Does guidance / session-start priming flip an agent's tool choice from
grep/cat to `repoctx`? Methodology + harness: [`README.md`](README.md).

## 2026-06-17 — madside, 10 tasks, **3 repeats** (N=30/arm), claude 2.1.170

The firm-up run (the N=1 below, averaged over nondeterminism). Same harness,
`REPEATS=3`; 30 runs per arm:

| arm | runs | used repoctx | repoctx/run | search/run | grep | read+native |
|---|---|---|---|---|---|---|
| **bare** (no guidance, no hook) | 30 | **20/30 (67%)** | 1.1 | 2.7 | 51 | 30 |
| **guidance** (skill + CLAUDE.md, no hook) | 30 | **27/30 (90%)** | 1.8 | 1.5 | 26 | 17 |
| **primed** (guidance + SessionStart `repoctx prime`) | 30 | **30/30 (100%)** | 1.9 | 1.2 | 23 | 11 |

### Reading

The bet **holds at N=3**: `used_repoctx` rises monotonically 67% → 90% → 100%,
`repoctx/run` 1.1 → 1.8 → 1.9, and the grep/read-native fallback falls
(grep 51 → 23, read+native 30 → 11). **Priming closes adoption to 100%** —
the headline claim replicates.

The bare arm is **higher than the N=1 (67% vs 30%)**: the test laptop's
*global* `~/.claude` carries the repoctx skill + SessionStart hook, which leaks
into the project-scope "bare" arm (only project guidance is stripped). So 67%
is a conservative floor, not a true grep baseline — the *gaps between arms* are
what the harness isolates, and those stay clean and monotonic.

### Second codebase (heartwood, Rust)

Planned (`tasks-heartwood.txt`, `TASKS_FILE` env) but **cut mid-run for token
budget** after madside completed. Not a null result — simply not collected.
Dogfooding it still paid off: indexing heartwood surfaced a real `overview`
crash on Rust `macro_rules!` callees, fixed in **v0.15.3**. Re-run when budget
allows: `REPEATS=3 TASKS_FILE=…/tasks-heartwood.txt run.sh …/heartwood`.

## 2026-06-17 — madside, 10 tasks, 1 repeat, claude 2.1.170

3 arms, headless `claude -p` runs, tallying the tool the agent *chose*:

| arm | runs | used repoctx | repoctx/run | search/run | grep | read+native |
|---|---|---|---|---|---|---|
| **bare** (no guidance, no hook) | 10 | **3/10** | 1.5 | 8.9 | 43 | 42 |
| **guidance** (skill + CLAUDE.md, no hook) | 10 | **6/10** | 0.8 | 1.8 | 13 | 5 |
| **primed** (guidance + SessionStart `repoctx prime`) | 10 | **10/10** | 1.4 | 1.9 | 15 | 4 |

### Reading

- **Bare is a grep loop.** Only 3/10 runs touched repoctx; ~9 grep/read
  calls per task. This is the baseline problem repoctx exists to fix.
- **Guidance does the heavy lifting on *reducing* grep** — search calls
  crash 8.9 → 1.8 — but adoption is **incomplete: 4/10 runs still never
  used repoctx**.
- **Priming closes the adoption gap: 10/10.** The SessionStart `prime`
  digest's marginal value over committed guidance is converting that
  holdout 40% into 100% — every session reaches for repoctx.

The bet (decision 2026-06-16-adoption-via-priming) **holds**: `used_repoctx`
rises monotonically 3 → 6 → 10 across bare → guidance → primed, and grep/read
collapses once any guidance is present.

### Caveats

One codebase (madside, TS), 10 tasks, a single repeat — directionally strong
(large, monotonic) but not statistically averaged over nondeterminism. The
bare arm's 3/10 is noisy (an agent occasionally probes `repoctx --help` even
unprompted). Re-run with `REPEATS=3` and a second codebase before treating the
exact numbers as precise. The *direction* — priming → full adoption — is clear.
