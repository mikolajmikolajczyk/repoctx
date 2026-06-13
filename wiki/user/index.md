# repoctx — user documentation

`repoctx` is an AI-oriented repository intelligence CLI: it indexes your repo once with Tree-sitter into a local SQLite store, then answers structural questions (`symbols`, `outline`, etc.) in milliseconds without re-reading whole files. Designed for AI coding agents — output defaults to [TOON](https://github.com/toon-format/toon) so the agent pays the fewest tokens for the same answer; `--json` switches to canonical JSON for scripts.

## Contents

- [Installation](installation.md) — pre-built binaries, Nix, or Cargo from source.
- [Quickstart](quickstart.md) — five-minute walk-through of every read command.
- [Commands reference](commands.md) — every command, every flag, exit codes.
- [Config — per-repo settings](config.md) — `repoctx config show / get / set / unset` over the `.repoctx/index.db` settings table.
- [Hook — per-agent install](hook.md) — `repoctx hook install <agent>` for Claude Code / Codex / opencode.
- [Output formats + agent integration](output-formats.md) — TOON vs JSON vs human; CLAUDE.md recipe, jq snippets.
- [Gain analytics](gain.md) — what `repoctx gain` measures, baseline rules, privacy stance.
- [Why repoctx saves tokens](why-repoctx.md) — the cost model + per-release [benchmark results](../bench/results.md).

## What lives where

Coding-agent docs (architecture, conventions, status) live under [`../agents/`](../agents/), not here.

## Project status

Pre-1.0 — see [`../agents/status.md`](../agents/status.md). The CLI surface (`index`, `symbols`, `outline`, `definition`, `context`, `status`, `hook`, `gain`) is end-to-end on Linux/macOS/Windows. An LSP-backed daemon is on the roadmap.

## License

LGPL-3.0-or-later. See [`../../LICENSE`](../../LICENSE).
