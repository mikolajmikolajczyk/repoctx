# Flag-aware hook rewrite

**Date**: 2026-06-14.

## Context

The transparent `rg`→`repoctx` rewrite only fired on a bare `rg <ident>`.
Agents reflexively add flags (`rg -n`, `rg -i`, `rg --type rust`,
`rg -C5`), and every flagged form fell through to the rtk-chain bypass →
real ripgrep, so repoctx was never offered. Measured: an agent building
the call-graph epic barely triggered the rewrite because it habitually
typed `rg -n`/`rg -l`.

## Decision

Extend `try_semantic_rewrite` with a flag-aware matcher. A curated
allowlist of **navigation flags** — those that change rg's *output*, not
the *intent of locating `<ident>`* — rewrite to a repoctx command:

- `-n` / `-l` / `-i` / `-w` / `-F` (and bundles like `-in`) → `symbols`
  (repoctx already returns file:line and is case-insensitive).
- `--type <t>` / `-t <t>` → `symbols --lang <slug>` (mapped; unknown type
  bails rather than guessing).
- `-A` / `-B` / `-C <n>` / `--context <n>` (incl. `-C5`) → `context
  --context <n>`.

Everything else passes through: regex, paths, multiple positionals,
quoted literals, and flags that change the *result set* (`-c`, `-v`,
`-o`, `--json`). Because a rewrite returns a repoctx command directly, it
also sidesteps the rtk-chain bypass — no corruption to dodge.

## Alternatives considered

- **Skill guidance only** — soft (probabilistic load + compliance);
  doesn't catch the reflexive flagged-`rg` habit. Kept as the complement
  (and the only channel for the call graph).
- **Rewrite every flag** — `-c`/`-v`/`-o` are textual/result-changing
  intents repoctx can't honor faithfully; excluded.

## The bet (and its escape hatch)

rg is textual, repoctx is symbolic. `rg -l foo` rewritten to `symbols`
misses files where `foo` appears only in a comment. This extends the bet
the bare-`rg` rewrite already made; mitigations: the `advisory` field
tells the agent to fall back to `rg`, and quoting (`rg "foo"`) bypasses.

## Trigger to revisit

If agents report missed textual matches often, narrow the allowlist
(drop `-l`) or gate behind full-coverage languages only.
