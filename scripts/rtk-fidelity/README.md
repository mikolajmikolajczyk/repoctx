# rtk fidelity canary

Does the rtk chain corrupt any command `repoctx` hands it?

`repoctx`'s hook chains most commands to [`rtk`](https://github.com/) for
output compression, and **bypasses the ones rtk mangles** (a denylist —
`is_chain_unsafe` in `crates/repoctx/src/hook_rewrite.rs`). That denylist is
correct for the corrupters we *know about*, but it can't auto-detect a *new*
rtk regression. This canary is the tripwire: run it on every rtk version bump.

**Manual only. Never wired into CI** — it depends on the locally installed
rtk version and on the probed tools being present.

```sh
scripts/rtk-fidelity/run.sh
```

## What it does

For each probe command it drives the **real production path** — pipes the
command through `repoctx hook claude --rtk-chain=1` and classifies the
decision:

| Decision | Meaning | Gated? |
|----------|---------|--------|
| `BYPASS`   | repoctx passes through → the real tool runs untouched (e.g. flagged `rg`). | no |
| `SEMANTIC` | repoctx rewrote it to a `repoctx <cmd>` (e.g. `rg foo` → `repoctx symbols`). repoctx owns the output. | no |
| `CHAIN`    | repoctx handed it to rtk. **The fidelity gate.** | yes |

For `CHAIN` commands it runs the rtk rewrite and the real command and
compares:

| Verdict | Trigger |
|---------|---------|
| `FAIL` | rtk produced **no** output (blank or the literal `(empty)`) while the real command produced some — the silent false-empty class that broke `ls` in rtk ≤0.41. |
| `WARN` | rtk output is < ¼ the real output **and** carries no truncation marker — a possible silent drop; eyeball it. |
| `PASS` | rtk output is non-empty and either comparable or clearly signals truncation (intended compression — the point of the chain). |

Exit is non-zero iff any `CHAIN` command `FAIL`s.

## Reading the output

A healthy rtk gives **0 fail, 0 warn**. A `FAIL` means rtk is silently losing
data on a command repoctx currently chains → add that command to
`is_chain_unsafe` and re-run. A `WARN` is a judgement call: faithful
compression (fine) vs silent drop (add to the denylist).

## Extending the probes

Edit the `probes=()` array in `run.sh` — one original command per line. Add
the proxies relevant to *your* environment (`docker`, `kubectl`, `gh`,
`cargo`, `pytest`, …) where those tools exist; the default list only covers
what a code-repo agent hits without extra infrastructure.

A leading `~` marks a proxy that intentionally compresses heavily and is not
line-comparable to the raw tool (e.g. `tree` is gitignore-aware while raw
`tree` walks `target/` and `.git/`). For those the ratio-`WARN` is skipped,
but the false-empty `FAIL` still applies.

## History

- rtk ≤0.41: `ls` returned `(empty)` for any directory (silent false-empty).
  Fixed in rtk 0.42.4 → `ls` chains again.
- flagged `rg` (`-i`/`--type`/`-g`): rtk forwards rg-only flags to GNU grep,
  losing recursive/gitignore. Still broken as of rtk 0.42.4 → bypassed.
