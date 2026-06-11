# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in Radicle issues (`rad issue list --all`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to Radicle issue filters instead.

## Works (as of v0.2.0, 2026-06-11)

CLI surface complete on Linux, macOS, and Windows. All 9 languages indexed.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx outline <file>` — document symbols for one file. Indented containment tree (human) or flat `{count, items}` (machine). Path arg accepts repo-relative or absolute; normalized through `to_db_path`. File-not-in-index → exit 1 with a prescriptive error.
- `repoctx definition <name>` — exact-name (case-sensitive) lookup over the workspace, kind-whitelisted to `{function, method, class, interface, type, module, macro, constant}`. `--lang`, `--limit` (default 50). Zero hits = exit 0, `count: 0`.
- `repoctx context <symbol>` — exact-name lookup (any kind) + the source window around each hit (`--context` lines either side, default 5; `--limit` matches, default 3). Reads source from disk and sets `stale: true` when the file's current `(mtime_ns, size)` differs from the indexed tuple. File deleted since indexing: warn and skip. Human mode prints a numbered listing per match; machine mode emits `{symbol, kind, location, before, body, after, stale}` rows.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- `repoctx gain` / `gain top` — token-savings analytics. Records every read command except `index`/`gain`/`hook`; aggregates only; `--since`, `--all`, `--history` window controls.
- `repoctx hook list` / `hook status` / `hook install <agent>` — per-agent install machinery for Claude Code / Codex / opencode. Pulls manifests + content from the GitHub mirror at a pinned git ref (default `v<binary version>`), caches under XDG (override via `REPOCTX_INTEGRATIONS_CACHE_DIR`). Three modes (`write`, `append`, `merge-section`). `--dry-run`/`--force`/`--ref`/`--no-cache` flags. No `uninstall` — install prints a per-file removal recipe.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive.
- Missing-index error uniform across read commands. Auto-index on by default; `--no-auto-index` opts out.
- Languages with full coverage: Go, Rust (struct/enum/union/type → `class` per upstream tags.scm), TypeScript (interface + abstract class + method_signature; plain class/function untagged upstream), TSX, JavaScript, Python, JSON, YAML (multi-doc), TOML (root pairs + `[table]` + `[[array]]`), Markdown (ATX + setext headings).

## Releases + CI

- **CI** (`.github/workflows/ci.yml`) — `fmt --check`, `build`, `test`, `clippy -D warnings`, `platform-check`. Three-OS matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`). Runs on every push to `main` + every PR.
- **Release** (`.github/workflows/release.yml`) — triggers on `v*` tag push. Matrix builds `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.

## Test coverage

70+ tests across the workspace (counts shift across releases; check `cargo test 2>&1 | grep -c "test result: ok"` for the live total). Breakdown by area:

- Store: unit + integration around schema migrations, BEGIN IMMEDIATE race, gain `usage` table API.
- Backend: serde-shape across `Symbol` / `Location` / `SymbolQuery`.
- Index: 11 parsing tests covering every language extractor (dedupe, custom Markdown/TOML queries, multi-doc YAML).
- Output: human / TOON / JSON over `List<T>`, gain reports, outline tree, context window.
- Repoctx CLI: per-command e2e (`index`, `symbols`, `status`, `outline`, `definition`, `context`, `gain`, `hook`) via `assert_cmd` against the release binary.
- Integrations: 23 unit (manifest parser, fetcher cache hit/miss, installer modes + idempotency).
- Hook e2e: 8 CLI-driven cases via `REPOCTX_INTEGRATIONS_CACHE_DIR` (no network).

## Performance baseline

2026-06-11, 5,000-file synthetic corpus, `scripts/bench.sh`:

- cold index: 318 ms (budget 10 s)
- no-op incremental: 50 ms (budget 300 ms)
- warm `symbols` query: 3 ms (budget 100 ms)
- `status --fast`: 5 ms (budget 50 ms)

All under their issue-948b131 budgets.

## In flight

`rad issue list --label state:in-progress` is the source of truth.

## Broken / regressions

None known.

## Not started

- Long-lived daemon + LSP backend — placeholder epic `58b45d5`. **Do not pre-empt.**
- Linux aarch64 / linux-musl release artifacts (current release workflow ships x86_64-gnu only on Linux).
- crates.io publish (deferred until API stabilizes; track CHANGELOG).

See `rad issue list` filtered by milestone.
