# Textually-complete search (`repoctx search`)

**Date**: 2026-06-14. **Epic**: `f4cb992`.

## Context

The hook rewrote `rg <ident>` → `repoctx symbols`, which only knows
*definitions*. Any textual occurrence that isn't a symbol — a mention in a
comment, a string literal, a doc — was silently dropped. The agent could
not tell repoctx had narrowed the result. rg is textual; repoctx was
symbolic; the rewrite lost the difference.

## Decision

Add `repoctx search <pattern>` and point the ambiguous-intent rewrites at
it instead of `symbols`.

- **Engine:** repoctx spawns **real ripgrep** in the repo (`--no-heading
  --line-number -m <per-file>`, gitignore-aware), parses `path:line:text`,
  and *also* queries the symbol index. repoctx owns the compression — this
  retires the dependency on rtk's (buggy) grep wrapper for these searches.
- **Output (symbol-led, textually complete):**
  `{ pattern, symbols: [Symbol], matches: { count, files: [{path, lines:
  [{line, text}], truncated}], truncated }, advisory }`. Definitions named
  `pattern` lead; every other textual match follows.
- **Compression caps:** ≤40 files, ≤8 matches/file, lines truncated at 200
  chars with `…`. Truncation is surfaced (`truncated` flags + advisory) so
  the agent knows to narrow or run `rg` directly. Returns `file:line`, never
  file dumps — far below rg-worst.
- **rg fallback:** ripgrep not on PATH → empty matches + advisory; symbol
  defs still returned.

## Rewrite reroute

| rg shape | → |
|----------|---|
| `rg foo`, `rg -n/-l/-i/-w/-F foo`, `grep -r foo .` | `repoctx search foo` |
| `rg --type L foo` | `repoctx search foo --lang L` |
| `rg -A/-B/-C N foo` | `repoctx context foo --context N` |
| `rg "fn foo"` / class / struct / function | `repoctx definition foo` |
| `-c` / `-v` / `-o` / `--json` / regex / paths / quoted literal | passthrough |

`repoctx symbols` / `definition` / `context` remain as explicit
symbol-only commands; only the *automatic* rewrite target changed.

## Bypass interaction

Flagged `rg` now mostly rewrites to `search` (served directly, no chain),
so the `is_chain_unsafe` flagged-rg bypass only covers the residual that
doesn't rewrite (`-c`/`-v`/`-o`/`-g`/regex/paths) — still correct.

## The bet

search returns more than pure-symbols (all matches), so it's larger — but
still far below opening every candidate file, and it never *loses* data.
Net: completeness at a modest token cost vs the lossy symbols rewrite.

## Trigger to revisit

If `search` output runs too large on big repos, lower the caps or add a
`--symbols-only` fast path.
