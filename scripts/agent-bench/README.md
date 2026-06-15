# Agent benchmark — `repoctx` vs `ripgrep`

How much token cost does `repoctx` save an AI coding agent on a real
codebase, vs the search-then-read workflow `ripgrep` alone gives it?

**Manual only. Never wired into CI** — clones are large, runs touch many
files. Run it when you want a number. Design, target repos (pinned SHAs),
query taxonomy, and pass/fail thresholds:
[`wiki/decisions/2026-06-13-agent-bench.md`](../../wiki/decisions/2026-06-13-agent-bench.md).

## Metric

Tokens are estimated at **4 bytes/token** on both sides — method-consistent
with `repoctx gain`. The tiny `tokens` binary
(`crates/bench-tokens`, `cargo build -p repoctx-bench-tokens`) does the
counting so the bats files only call shell — no Python, no tokenizer dep.

For each query the harness compares:

| Path | Tokens charged |
|---|---|
| `repoctx <cmd> --json` | tokens in its stdout |
| ripgrep (worst) | `rg` match output + bytes/4 of **every** candidate file |
| ripgrep (best) | `rg` match output + bytes/4 of the **top** match file |

## Run

```sh
# from the repo root
scripts/agent-bench/run.sh --clone     # build + clone pinned repos + run
scripts/agent-bench/run.sh             # build + run (reuse existing clones)
```

Requires [`bats`](https://github.com/bats-core/bats-core), `rg`, `git`,
`cargo`. The driver builds `repoctx` + `tokens` (release), clones the
pinned targets into `$BENCH_CLONES` (default `/tmp/repoctx-bench`), runs
`smoke.bats` (helper self-test), then any per-repo suite whose `.bats`
file + clone are present.

## Layout

| File | Role |
|---|---|
| `lib/helpers.bash` | `repoctx_tokens`, `rg_worst_tokens`, `rg_best_tokens`, `savings_pct`, `assert_savings_above`, `assert_advisory_present`/`assert_no_advisory` |
| `smoke.bats` | helper self-test against a tiny in-test fixture (no clone) |
| `run.sh` | build + clone + run driver |
| `report.sh` | emit the per-query number table (feeds `wiki/bench/results.md`) |
| `<repo>.bats` | per-repo suites: helix / rust-analyzer / vuejs-core / **linux** (C scale + call-graph target) |

All suites cover every command family: `definition` / `context` / `symbols` /
`outline` / `search` / `callers` / `callees` / `callgraph`. Latest numbers
(incl. the Linux kernel) live in [`wiki/bench/results.md`](../../wiki/bench/results.md).

## Writing a suite

A per-repo `.bats` file `load`s the helpers and asserts against
`$BENCH_REPO` (set by `run.sh`):

```bash
load lib/helpers
@test "definition: Selection" {
  rc="$(repoctx_tokens "$BENCH_REPO" definition Selection)"
  rg="$(rg_worst_tokens "$BENCH_REPO" Selection)"
  assert_savings_above "$rc" "$rg" 80
}
```
