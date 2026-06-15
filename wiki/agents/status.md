# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in GitHub issues (`gh issue list`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to GitHub issue filters instead.

## Works (as of v0.8.0, 2026-06-15)

CLI surface complete on Linux, macOS, and Windows. 20 languages indexed (16 full coverage + 4 partial). `repoctx init` wires repoctx into Claude Code as the sole PreToolUse hook (a committed `.repoctx/hook.sh` dumb-pipe script + in-binary rewrite/chain), chaining rtk underneath with race detection, `hook doctor` drift repair, and `--uninstall`. The hook rewrites `rg <ident>` and navigation-flag variants (`-n`/`-l`/`-i`/`-w`/`-F`/`--type`/`-A/-B/-C`) to `repoctx search`/`context` (v0.8.0); residual flagged `rg` rtk can't handle (`-c`/`-v`/`-o`/`-g`, any pipeline segment) bypasses the chain so the agent's real ripgrep runs. Integration content is embedded in the binary (no network). Per-repo config layer + agent-coverage advisory live. `gain` token figures are bytes/4 estimates (method-consistent ratio); precise BPE counting lives in the bench suite — the agent benchmark harness (`scripts/agent-bench/`) + results page (`wiki/bench/results.md`) gate token savings on three real codebases.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx search <pattern>` (v0.8.0) — textually-complete search: exact-name symbol definitions **+** every textual match from real ripgrep, compressed to `file:line` (caps: 40 files, 8/file, 200-char lines; truncation flagged). repoctx owns the compression; rg-absent → symbol-only + advisory. What the hook rewrites `rg <ident>` to.
- `repoctx callers <name>` / `callees <name>` / `callgraph <name>` (v0.8.0) — static, name-based call graph for the core-8 langs (Rust/Python/JS/TS/Go/C/C++/Java); ADR-0010; schema v4 `calls` table; callee resolution at query time; ambiguous/unresolved edges flagged + advised. `callgraph` adds `--depth` (default 3) + `--direction up|down|both`, cycle-safe.
- `repoctx deps <file>` / `rdeps <module>` / `boundary --from --to` (unreleased, epic #4) — import / dependency graph for the core-8 langs; ADR-0011; schema v5 `imports` table. `deps` lists a file's import specifiers; `rdeps` finds importers by substring; `boundary` lists crossings where files under `--from` import `--to` ("does layer A import B?"), `--forbid` makes it a CI gate. String-based, query-time resolution. Remaining #4 children: import-cycle detection, module map + public API surface.
- `repoctx outline <file>` — document symbols for one file. Indented containment tree (human) or flat `{count, items}` (machine). Path arg accepts repo-relative or absolute; normalized through `to_db_path`. File-not-in-index → exit 1 with a prescriptive error.
- `repoctx definition <name>` — exact-name (case-sensitive) lookup over the workspace, kind-whitelisted to `{function, method, class, interface, type, module, macro, constant}`. `--lang`, `--limit` (default 50). Zero hits = exit 0, `count: 0`.
- `repoctx context <symbol>` — exact-name lookup (any kind) + the source window around each hit (`--context` lines either side, default 5; `--limit` matches, default 3). Reads source from disk and sets `stale: true` when the file's current `(mtime_ns, size)` differs from the indexed tuple. File deleted since indexing: warn and skip. Human mode prints a numbered listing per match; machine mode emits `{symbol, kind, location, before, body, after, stale}` rows.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- `repoctx gain` / `gain top` — token-savings analytics. Records every read command except `index`/`gain`/`hook`; aggregates only; `--since`, `--all`, `--history` window controls.
- `repoctx discover` (unreleased, issue #7) — hook passthrough telemetry. The hook records every grep/rg/find by idiom (bare-ident/literal-string/regex/call-shape/import-shape/…) + outcome (rewritten/passthrough/chained); `discover` ranks the adoption gap. Aggregate-only (no command bodies), best-effort, gated on an existing index DB, opt-out via `hook.telemetry`. Schema v6 `hook_events` table. Drives which grep idioms the hook should learn to rewrite next.
- `repoctx hook list` / `hook status` / `hook install <agent>` — per-agent install machinery for Claude Code / Codex / opencode. Manifests + content are embedded in the binary (`include_str!`) — install works offline and is version-locked to the binary. Three modes (`write`, `append`, `merge-section`). `--dir`/`--dry-run`/`--force` flags. No `uninstall` — install prints a per-file removal recipe.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive. Default format also configurable via `output.default` in the per-repo settings table.
- `repoctx config show/get/set/unset` — per-repo settings (`hook.rewrite`, `hook.use_rtk`, `hook.chainable`, `gain.no_record`, `gain.record_query`, `output.default`, `index.nested_keys`; plus read-only `hook.script_path`). Stored in the `.repoctx/index.db` settings table (schema v4). Precedence: CLI flag → env var → settings → default.
- `repoctx init [-g]` — the onboarding command. Writes a committed `.repoctx/hook.sh` (dumb-pipe, no jq), points `.claude/settings.json`'s sole PreToolUse → Bash entry at it, writes `.gitattributes`, installs SKILL.md + CLAUDE.md guidance. `--rtk auto|on|off`, `--yes`, `--force`, `--dry-run`. `--uninstall [--restore-backup]` reverses it. Refuses race-prone configs (foreign hook anywhere, or a global rtk / project-vs-global repoctx scope clash) unless `--force` — except a project `init` under an existing global repoctx hook, which installs **guidance only** (skill + CLAUDE.md) and skips the redundant project hook.
- `repoctx hook claude [--rtk-chain=0|1]` — PreToolUse handler. Rewrites recognized `rg`/`grep <identifier>` patterns to `repoctx symbols`/`definition --json`; on passthrough, chains the first allowlisted tool on PATH (`hook.chainable`, default rtk) and forwards its output. `hook.rewrite = off` disables semantic rewrites; `force` relaxes the parser.
- `repoctx hook doctor [-g] [--fix]` — re-renders the expected script + compares to disk (structural drift, ignoring config value lines), checks the settings entry, reports foreign hooks; `--fix` regenerates + restores with a backup. Exits 1 on issues without `--fix`.
- `repoctx hook list / status / install <agent>` — embedded per-agent guidance install (offline; `install` is the low-level primitive used for codex/opencode).
- `repoctx rewrite <cmd>` — show the hook's rewrite decision (exit 0 + rewritten command, or 1 = passthrough).
- No missing-index error surface for users — read commands always build the DB if needed and incrementally reindex changed files before answering.
- `repoctx languages` — surfaces the per-language coverage matrix; read commands attach an `advisory` field to machine output when the query underperforms because of language coverage limits. Agents fall back to `rg` when present.
- Languages with full coverage: Go, Rust, TypeScript + TSX, JavaScript, Python, Markdown, and the v0.7.0 batch — Ruby, C, C++, Java, C#, PHP, Lua, Kotlin, Swift (upstream `tags.scm` where shipped; vendored minimal queries for Kotlin; Swift captures struct/func/method but not class names).
- Languages with partial coverage: JSON / YAML / TOML (top-level keys by default; `index.nested_keys = true` opts into all-depth key extraction), and Bash (function definitions only). The advisory layer warns + suggests `rg` for exhaustive search.

## Releases + CI

- **CI** (`.github/workflows/ci.yml`) — `fmt --check`, `build`, `test`, `clippy -D warnings`, `platform-check`. Three-OS matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`). Runs on every push to `main` + every PR.
- **Release** (`.github/workflows/release.yml`) — triggers on `v*` tag push. Matrix builds `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.

## Test coverage

~28 workspace test suites green (live total: `cargo test 2>&1 | grep -c "test result: ok"`). Notable areas:

- **Meta-hook**: `init` (install / global / rtk-displacement / migration / race refusal / dry-run), `doctor` (drift detect + `--fix`), `--uninstall` (entry/script removal, foreign-preserve, restore-backup), and `hook_script_e2e` running the rendered `hook.sh` under bash across the RTK_CHAIN × repoctx-present × rtk-present matrix.
- **Correctness suite** (CI-gated): rewrite-decision corpus (≥100 rows, both entry points + per-rule coverage) and accuracy parity vs ripgrep across 10 language fixtures with a known-symbol sidecar.
- Hook rewrite parser, `hook_marker` reader, `hook_scan` classify + race ruleset, config round-trip/precedence, advisory generation, output format snapshots, TS/TSX vendored tags regression.

## Performance baseline

2026-06-11, 5,000-file synthetic corpus, `scripts/bench.sh`:

- cold index: 318 ms (budget 10 s)
- no-op incremental: 50 ms (budget 300 ms)
- warm `symbols` query: 3 ms (budget 100 ms)
- `status --fast`: 5 ms (budget 50 ms)

All under their release-issue budgets.

## In flight

`gh issue list --label state:in-progress` is the source of truth.

- **Call graph — remaining languages** (`#1`): the call graph ships for the core 8; call-site queries for the other indexed langs (Ruby/C#/PHP/Lua/Kotlin/Swift/Bash) are the open follow-up.
- **Import / dependency graph** (`#4`, epic): `deps`/`rdeps`/`boundary` landed (schema v5, core-8 import extraction, ADR-0011, boundary/layering check with `--forbid` CI gate); remaining children — import-cycle detection, module dependency map + public API surface.

## Broken / regressions

None known.

## Not started

- Long-lived daemon + LSP backend — placeholder epic. **Do not pre-empt.**
- Linux aarch64 / linux-musl release artifacts (current release workflow ships x86_64-gnu only on Linux).
- crates.io publish (deferred until API stabilizes; track CHANGELOG).

See `gh issue list` filtered by the `milestone:*` label.
