# repoctx — user documentation

`repoctx` is an AI-oriented repository intelligence CLI: it indexes your repo once with Tree-sitter into a local SQLite store, then answers structural questions (`symbols`, `outline`, etc.) in milliseconds without re-reading whole files. Designed for AI coding agents — output defaults to [TOON](https://github.com/toon-format/toon) so the agent pays the fewest tokens for the same answer; `--json` switches to canonical JSON for scripts.

## Contents

- [Installation](installation.md) — Nix (recommended) or plain Cargo.
- [Quickstart](quickstart.md) — `index` → `symbols` → `status` → `gain` in five minutes.
- [Commands reference](commands.md) — full M0 + M1 surface, flags, exit codes.
- [Hook — per-agent install](hook.md) — `repoctx hook install <agent>` for Claude Code / Codex / opencode.
- [Output formats + agent integration](output-formats.md) — TOON vs JSON vs human; CLAUDE.md recipe, jq snippets.
- [Gain analytics](gain.md) — what `repoctx gain` measures, baseline rules, privacy stance.

## What lives where

Coding-agent docs (architecture, conventions, status) live under [`../agents/`](../agents/), not here.

## Project status

Pre-1.0 — see [`../agents/status.md`](../agents/status.md). M0 + M1 functional surface (`index`, `symbols`, `outline`, `definition`, `context`, `status`, `gain`, `hook`) is end-to-end on Linux/macOS/Windows. M2 (LSP daemon) is on the roadmap.

## License

LGPL-3.0-or-later. See [`../../LICENSE`](../../LICENSE).
