# Gain analytics

> **`repoctx` does not replace grep. It reduces the amount of code an AI agent has to read after searching.**

A naive "tokens emitted by `repoctx` vs tokens emitted by `rg`" comparison can lose — for a single-symbol query, `rg` may return fewer characters. But that ignores the actual cost: after `rg`, the agent still has to open the candidate files and read hundreds or thousands of lines to find the answer. `repoctx symbols` returns the symbol *and* its precise location, so that next step doesn't happen.

`repoctx gain` quantifies that avoided navigation, per invocation.

## What `repoctx gain` shows

After four `repoctx symbols` queries against this repo:

```sh
repoctx gain
```

```text
Last 30 days

Commands:
  4

Returned:
  1,084 tokens

Estimated baseline:
  12,868 tokens

Reduction:
  91.6%

Estimated savings:
  11.8K tokens
```

Field by field:

- **Last 30 days** — default window. Override with `--since 7d` / `--since 2h` / `--since 30m` / `--since 120s`, or `--all` for unbounded.
- **Commands** — number of recorded invocations (read commands only — `index` and `gain` are not recorded).
- **Returned tokens** — tokens in what `repoctx` actually printed, counted with [tiktoken-rs](https://crates.io/crates/tiktoken-rs) `cl100k_base`.
- **Estimated baseline** — what an agent would have had to ingest if it grep'd for the same answer and then read every candidate file end to end. Computed from the index alone (no extra IO) as `Σ files.size / 4`.
- **Reduction** — `(baseline − returned) / baseline`, rendered to one decimal.
- **Estimated savings** — `baseline − returned`, abbreviated for big numbers (`11.8K`, `4.1M`).

If the window has no recorded invocations, `gain` prints zeros and exits 0 with a `no recorded invocations in window` line. `--history N` swaps the summary for the N most recent rows (default 20 if no number is given).

## `repoctx gain top`

Per-command ranking. Default ordering is by absolute `estimated_savings` (where the wins actually live); `--by ratio` switches to reduction percentage. Tiebreak is command name.

```sh
repoctx gain top
```

```text
Last 30 days
by: saved

symbols:
  91.6% reduction · 11.8K tokens saved · 4 call(s)
```

With M1's `outline` / `definition` / `context` recording alongside `symbols`, this view tells you which command is doing the most work for you. Example after a mixed session:

```text
context:
  78.4% reduction · 9.9K tokens saved · 9 call(s)
outline:
  77.3% reduction · 8.1K tokens saved · 6 call(s)
definition:
  96.0% reduction · 5.0K tokens saved · 5 call(s)
symbols:
  91.6% reduction · 11.8K tokens saved · 4 call(s)
```

## Baseline per command

| Command | Candidates = files whose `size` is summed | Baseline tokens |
|---|---|---|
| `symbols <q>` | Files containing at least one matching symbol | `Σ files.size / 4` |
| `outline <file>` | The single file | `files.size / 4` |
| `definition <name>` | Files containing at least one hit | `Σ files.size / 4` |
| `context <name>` | Files containing the matched symbols | `Σ files.size / 4` |
| `status` | not recorded | — |
| `index` | not recorded | — |
| `gain` / `gain top` | not recorded | — |
| `hook list` / `status` / `install` | not recorded | — |

The `bytes / 4` divisor is the standard rough approximation for English-like text under cl100k_base. We deliberately do NOT re-tokenize candidate files at record time — that would eat the very IO `repoctx` is meant to avoid. Decision and revisit triggers: [`../decisions/2026-06-11-gain-tokenizer-and-baseline.md`](../decisions/2026-06-11-gain-tokenizer-and-baseline.md).

## What is recorded — and what is not

Every read command appends one row to a `usage` table inside `.repoctx/index.db`. The recorded columns are:

| Column | Stored value |
|---|---|
| `ts_unix_ns` | Invocation timestamp |
| `command` | `"symbols"`, `"outline"`, `"definition"`, `"context"` |
| `candidate_files` | Number of files that contributed to the baseline |
| `candidate_bytes` | Their summed size in bytes |
| `estimated_baseline_tokens` | `candidate_bytes / 4` |
| `returned_tokens` | Tokens in what `repoctx` actually printed |
| `output_format` | `"human"`, `"toon"`, or `"json"` |
| `query` | **NULL** unless you opted in with `--record-query` |

What is **NOT** stored, by default:

- Filenames, directories, or paths.
- Symbol names.
- File contents or any source code.
- Your query string (unless `--record-query`).

Filenames are touched transiently at record time so we can sum `files.size` for the candidate path list, but only the aggregate (`candidate_files`, `candidate_bytes`) lands in the row. The integration test `privacy_no_filenames_or_symbol_names_in_usage_table` dumps the table and asserts no leakage.

Nothing leaves the machine — there is no network call anywhere on the gain path.

## Privacy switches

| Switch | Effect |
|---|---|
| `--no-record` | Skip recording for this invocation only. |
| `RUST_REPOCTX_NO_RECORD=1` | Skip recording for every invocation in the shell. |
| `--record-query` | Opt in to persisting the query string on this invocation. |

## Reading and resetting

`.repoctx/index.db` is plain SQLite — open it with `sqlite3` whenever you want a closer look:

```sh
sqlite3 .repoctx/index.db 'SELECT ts_unix_ns, command, candidate_files, returned_tokens FROM usage ORDER BY ts_unix_ns DESC LIMIT 10'
```

To start counting fresh:

```sh
sqlite3 .repoctx/index.db 'DELETE FROM usage'
```

(There's no dedicated `repoctx gain reset` command in M0 — `DELETE FROM usage` is the supported path; the table is recreated automatically on the next recorded invocation.)

## See also

- [ADR-0008 — TOON as default machine output](../adr/0008-toon-default-machine-output.md) (token-cost framing)
- [`../decisions/2026-06-11-gain-tokenizer-and-baseline.md`](../decisions/2026-06-11-gain-tokenizer-and-baseline.md)
