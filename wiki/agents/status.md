# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in Radicle issues (`rad issue list --all`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to Radicle issue filters instead.

## Works (as of v0.5.1, 2026-06-12)

CLI surface complete on Linux, macOS, and Windows. 9 languages indexed (7 full coverage + 3 partial). Claude Code transparent rewrite hook + per-repo config layer + agent-coverage advisory all live.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx outline <file>` — document symbols for one file. Indented containment tree (human) or flat `{count, items}` (machine). Path arg accepts repo-relative or absolute; normalized through `to_db_path`. File-not-in-index → exit 1 with a prescriptive error.
- `repoctx definition <name>` — exact-name (case-sensitive) lookup over the workspace, kind-whitelisted to `{function, method, class, interface, type, module, macro, constant}`. `--lang`, `--limit` (default 50). Zero hits = exit 0, `count: 0`.
- `repoctx context <symbol>` — exact-name lookup (any kind) + the source window around each hit (`--context` lines either side, default 5; `--limit` matches, default 3). Reads source from disk and sets `stale: true` when the file's current `(mtime_ns, size)` differs from the indexed tuple. File deleted since indexing: warn and skip. Human mode prints a numbered listing per match; machine mode emits `{symbol, kind, location, before, body, after, stale}` rows.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- `repoctx gain` / `gain top` — token-savings analytics. Records every read command except `index`/`gain`/`hook`; aggregates only; `--since`, `--all`, `--history` window controls.
- `repoctx hook list` / `hook status` / `hook install <agent>` — per-agent install machinery for Claude Code / Codex / opencode. Pulls manifests + content from the GitHub mirror at a pinned git ref (default `v<binary version>`), caches under XDG (override via `REPOCTX_INTEGRATIONS_CACHE_DIR`). Three modes (`write`, `append`, `merge-section`). `--dry-run`/`--force`/`--ref`/`--no-cache` flags. No `uninstall` — install prints a per-file removal recipe.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive. Default format also configurable via `output.default` in the per-repo settings table.
- `repoctx config show/get/set/unset` — per-repo settings (`hook.rewrite`, `hook.ref`, `hook.no_cache`, `hook.chain_commands`, `gain.no_record`, `gain.record_query`, `output.default`). Stored in `.repoctx/index.db` schema v3 settings table. Precedence: CLI flag → env var → settings → default.
- `repoctx hook claude` — PreToolUse hook handler. Rewrites recognized `rg`/`grep <identifier>` patterns to `repoctx symbols`/`definition --json`; chains through commands saved in `hook.chain_commands` on passthrough. `hook.rewrite = off` disables semantic rewrites (pure chain proxy); `force` relaxes the parser.
- `repoctx hook install claude` takes ownership of `.claude/settings.json` PreToolUse → Bash matcher (displaces any prior entries into the chain). `repoctx hook doctor` re-runs the takeover idempotently to recover from sibling installers overwriting our entry. Both also scan `~/.claude/settings.json` and warn (read-only) when a user-global tool (e.g. `rtk init -g`) would parallel-race our project-local entry — Claude Code merges hooks across all scopes by design.
- No missing-index error surface for users — read commands always build the DB if needed and incrementally reindex changed files before answering.
- `repoctx languages` — surfaces the per-language coverage matrix; read commands attach an `advisory` field to machine output when the query underperforms because of language coverage limits. Agents fall back to `rg` when present.
- Languages with full coverage: Go, Rust (struct/enum/union/type → `class` per upstream tags.scm), TypeScript + TSX (full coverage via vendored Aider tags.scm: plain class, plain function, arrow function, method, type alias, enum, interface, abstract class — Apache-2.0), JavaScript, Python, Markdown (ATX + setext headings).
- Languages with partial coverage: JSON (top-level keys), YAML (top-level keys of each document, multi-doc), TOML (root pairs + `[table]` + `[[array]]` headers). Nested keys / sections inside tables are not surfaced; the advisory layer warns and suggests `rg` as a fallback.

## Releases + CI

- **CI** (`.github/workflows/ci.yml`) — `fmt --check`, `build`, `test`, `clippy -D warnings`, `platform-check`. Three-OS matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`). Runs on every push to `main` + every PR.
- **Release** (`.github/workflows/release.yml`) — triggers on `v*` tag push. Matrix builds `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.

## Test coverage

21 workspace test suites green (live total: `cargo test 2>&1 | grep -c "test result: ok"`). New areas since v0.2.0:

- Hook rewrite: 13 unit tests for the rewrite-rule parser (single-ident matchers, quoted-pattern routing, shell metacharacter refusal, regex passthrough).
- Hook takeover: 11 unit tests for project-local `.claude/settings.json` ownership + user-global scan + warn paths.
- Config: 7 round-trip + precedence unit tests (CLI > env > settings > default), settings.json hand-edit fallback.
- Output: 3 new resolve() tests covering the `output.default` config layer.
- Advisory: 8 advisory generation tests (full/partial language paths, empty-workspace + lang-filter combos).
- Languages: TS/TSX vendored Aider tags.scm regression tests covering plain class / function / arrow / type alias / enum.

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
