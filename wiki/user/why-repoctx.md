# Why repoctx saves tokens

The short version: an agent answering "where is X / show me X / what's
in this file" with `ripgrep` pays for **every byte of every file it
opens to disambiguate**. `repoctx` answers the same question with a
structured record — a location, a kind, a small code window — so the
agent never opens the haystack.

## The loop it replaces

```
1. rg <name>            → N line-matches across M files
2. Read each candidate  → pay tokens for every byte of every file
3. Pick the real one
```

`repoctx` collapses this to one query whose output is already the
answer:

```sh
$ repoctx definition parse_config --json
{"path":"crates/repoctx/src/config.rs","line":42,"symbol":"parse_config","kind":"function"}
```

## How we measure it

We price three ways to satisfy one agent intent, in the same unit:

- **repoctx** — tokens in `repoctx <cmd> --json` stdout.
- **rg-worst** — `rg` match output + every candidate file the pattern
  hits (the agent opens all of them).
- **rg-best** — `rg` match output + just the single top-match file (the
  agent guesses right first try).

Tokens are counted at **4 bytes/token** on both sides — the same metric
as [`repoctx gain`](gain.md). No model-specific tokenizer, so the
numbers don't drift with whatever BPE a given model ships; they're
reproducible from a clone + the binary.

## The numbers

Measured on three SHA-pinned real codebases. Typical savings:

| Command family | What it returns | Savings vs rg-worst |
|---|---|---|
| `definition` | one location record | ~99% |
| `symbols` / `outline` | a structured list | ~99% |
| `context` | the actual source window | 65–99% |

`context` is lower on purpose: it *returns the code* the agent asked to
see, so it can't compress like a pointer. Even then it beats opening the
files by hand.

The full per-query table, per release, with the pinned SHAs:
[benchmark results](../bench/results.md). The methodology and pass/fail
thresholds: [agent-bench design doc](../decisions/2026-06-13-agent-bench.md).

## Reproduce it yourself

```sh
scripts/agent-bench/run.sh --clone     # build, clone pinned repos, run gates
BENCH_CLONES=/tmp/repoctx-bench \
  scripts/agent-bench/report.sh        # print the number table
```

Requires `bats`, `rg`, `git`, `cargo` (all in the Nix devShell).
