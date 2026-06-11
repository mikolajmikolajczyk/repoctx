# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in Radicle issues (`rad issue list --all`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to Radicle issue filters instead.

## Works (as of 2026-06-11)

M0 functional surface, all 9 languages indexed.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive.
- Missing-index error uniform across read commands: `no index found — run 'repoctx index'`, exit 1, empty stdout.
- Languages with full coverage: Go, Rust (struct/enum/union/type → `class` per upstream tags.scm), TypeScript (interface + abstract class + method_signature; plain class/function untagged upstream), TSX, JavaScript, Python, JSON, YAML (multi-doc), TOML (root pairs + `[table]` + `[[array]]`), Markdown (ATX + setext headings).

Test coverage: 56 tests across the workspace (5 store unit + 11 store integration + 7 backend serde-shape + 5 output + 11 index parsing + repo_root unit + 7 index_cmd e2e + 8 symbols_cmd e2e + 4 status_cmd e2e + 5 combined e2e).

## In flight

`rad issue list --label state:in-progress` is the source of truth. None active at the close of M0 functional work.

## Broken / regressions

None known.

## Not started

- Non-functional M0 hardening: CI (`72acdcc`), concurrency/corruption (`da5f6cc`), platform-agnostic enforcement (`bb6d7f7`).
- M1 navigation commands (`outline`, `definition`, `context`) — epic `8ce08ce`.
- Gain analytics — epic `4dd57c8`.
- User docs — epic `4fc80f5`.
- M2 daemon + LSP — placeholder epic `58b45d5`. **Do not pre-empt.**

See `rad issue list` filtered by milestone.
