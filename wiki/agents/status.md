# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in Radicle issues (`rad issue list --all`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to Radicle issue filters instead.

## Works (as of 2026-06-11)

M0 functional surface, all 9 languages indexed.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx outline <file>` — document symbols for one file. Indented containment tree (human) or flat `{count, items}` (machine). Path arg accepts repo-relative or absolute; normalized through `to_db_path`. File-not-in-index → exit 1 with a prescriptive error.
- `repoctx definition <name>` — exact-name (case-sensitive) lookup over the workspace, kind-whitelisted to `{function, method, class, interface, type, module, macro, constant}`. `--lang`, `--limit` (default 50). Zero hits = exit 0, `count: 0`.
- `repoctx context <symbol>` — exact-name lookup (any kind) + the source window around each hit (`--context` lines either side, default 5; `--limit` matches, default 3). Reads source from disk and sets `stale: true` when the file's current `(mtime_ns, size)` differs from the indexed tuple. File deleted since indexing: warn and skip. Human mode prints a numbered listing per match; machine mode emits `{symbol, kind, location, before, body, after, stale}` rows.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- `repoctx hook list` / `hook status` / `hook install <agent>` — per-agent install machinery for Claude Code / Codex / opencode. Pulls manifests + content from the GitHub mirror at a pinned git ref (default `v<binary version>`), caches under XDG. Three modes (`write`, `append`, `merge-section`). `--dry-run`/`--force`/`--ref`/`--no-cache` flags. No `uninstall` — install prints a per-file removal recipe.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive.
- Missing-index error uniform across read commands: `no index found — run 'repoctx index'`, exit 1, empty stdout.
- Languages with full coverage: Go, Rust (struct/enum/union/type → `class` per upstream tags.scm), TypeScript (interface + abstract class + method_signature; plain class/function untagged upstream), TSX, JavaScript, Python, JSON, YAML (multi-doc), TOML (root pairs + `[table]` + `[[array]]`), Markdown (ATX + setext headings).

Test coverage: 56 tests across the workspace (5 store unit + 11 store integration + 7 backend serde-shape + 5 output + 11 index parsing + repo_root unit + 7 index_cmd e2e + 8 symbols_cmd e2e + 4 status_cmd e2e + 5 combined e2e).

Performance baseline (2026-06-11, 5,000-file synthetic corpus, `scripts/bench.sh`):
cold index 318 ms, no-op incremental 50 ms, warm `symbols` query 3 ms, `status --fast` 5 ms — all well under their issue-948b131 budgets (10 s / 300 ms / 100 ms / 50 ms).

## In flight

`rad issue list --label state:in-progress` is the source of truth. None active at the close of M0 functional work.

## Broken / regressions

None known.

## Not started

- Release engineering (`bc9da7c`) + README polish (`c14348e`).
- M1 navigation commands — epic `8ce08ce` landed (outline + definition + context + gain-wire + docs).
- M1.5 integrations — epic `b497f7f` landed (`repoctx hook` for claude/codex/opencode).
- M2 daemon + LSP — placeholder epic `58b45d5`. **Do not pre-empt.**

See `rad issue list` filtered by milestone.
