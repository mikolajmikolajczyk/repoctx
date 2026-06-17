# Status

Snapshot of what works, what's in flight, what's broken. **Not the roadmap** — roadmap lives in GitHub issues (`gh issue list`).

Update this when a feature lands, breaks, or gets pulled. Stale status is worse than no status — if you can't keep it fresh, link straight to GitHub issue filters instead.

## Works (as of v0.11.4, 2026-06-17)

CLI surface complete on Linux, macOS, and Windows. 20 languages indexed (16 full coverage + 4 partial). `repoctx init` is the single onboarding command: it installs the agent guidance files and, for Claude, wires a **SessionStart** hook that runs `repoctx prime` so the orientation digest is injected into the agent's context at session start. Adoption is via **priming** — the agent is oriented once per session, not by intercepting every command (the per-command PreToolUse rewrite hook, rtk chaining, `discover` telemetry, and `repoctx hook`/`rewrite` commands were removed; decisions `2026-06-16-adoption-via-priming` + `2026-06-17-remove-pretooluse-hook`). A user's own `rtk` hook is independent of repoctx. Integration content is embedded in the binary (no network). Per-repo config layer + agent-coverage advisory live. `gain` token figures are bytes/4 estimates (method-consistent ratio); precise BPE counting lives in the bench suite — the agent benchmark harness (`scripts/agent-bench/`) + results page (`wiki/bench/results.md`) gate token savings on three real codebases.

- `repoctx index` — incremental walk + Tree-sitter parse + SQLite upsert; rayon parses, single sequential writer; skip rules per epic contract (gitignored, `> 2 MiB`, non-UTF-8, `.git`, `.repoctx`); `--force` reparses all; deleted files pruned. ~80 ms cold / ~7 ms no-op on this repo.
- `repoctx symbols <query>` — case-insensitive substring across the index; `--kind`, `--lang`, `--limit` filters; deterministic `ORDER BY name COLLATE NOCASE, file_path, start_line`; empty result = exit 0 + `count: 0`.
- `repoctx search <pattern>` (v0.8.0) — textually-complete search: exact-name symbol definitions **+** every textual match from real ripgrep, compressed to `file:line` (caps: 40 files, 8/file, 200-char lines; truncation flagged). repoctx owns the compression; rg-absent → symbol-only + advisory.
- `repoctx callers <name>` / `callees <name>` / `callgraph <name>` (v0.8.0) — static, name-based call graph for the core-8 langs (Rust/Python/JS/TS/Go/C/C++/Java); ADR-0010; `calls` table; callee resolution at query time, **receiver-aware** (schema v9 `is_method`: `obj.foo()` binds only to a `method`, never a free `function`; builtin method names guarded — issue #9, decision 2026-06-16-receiver-aware-call-resolution); ambiguous/unresolved edges flagged + advised. `callgraph` adds `--depth` (default 3) + `--direction up|down|both`, cycle-safe.
- `repoctx deadcode` / `impact <name>` / `cycles` (unreleased, issue #3) — Tier-1 analyses over the existing `calls` table: uncalled function/method defs (entry points excluded), transitive-caller blast radius, and call-cycle detection. Name-based (ADR-0010), advisory, no new indexing.
- `repoctx changed [--since REF]` (unreleased, issue #6) — change-aware blast radius: git diff → changed symbols → transitive callers (reverse BFS, name-based). Tracked files only; for code review.
- `repoctx overview` (unreleased, issue #5) — repo architecture in one call: totals (code vs doc/config split), per-language, per-directory module sizes **ranked by code symbols** (#9-D), entry points (`main` + JS/TS bootstraps), hotspots (receiver-aware), and **public API surface** (exported symbols per module — #10; lexical visibility for Go/Rust/JS/TS, others `unknown`). Composes index + call graph; no new extraction.
- `repoctx communities` (unreleased, issue #14) — call-graph clustering: single-level Louvain modularity over the **resolved** call graph (unambiguous, single-callable-def callees) → subsystems labeled by highest-degree member, plus god nodes (highest-degree symbols overall). Hand-rolled Louvain (plain adjacency, not petgraph); topology-only, name-based (ADR-0010); capped output (30 clusters / 15 members / 15 god nodes). Orientation layer for unfamiliar repos. Graph nodes keyed per-definition `(name, file, line)` — same-named defs stay distinct (qualified `name@file:line`); host/builtin method names excluded from degree (decision 2026-06-16-graph-node-identity).
- `repoctx report` (unreleased, issue #15) — deterministic one-page architecture report (markdown), generated from topology only (no LLM, no network): god nodes, subsystems (#14), cross-cluster bridges (inter-cluster edges ranked by combined degree), entry points, and templated suggested questions. Human render *is* the markdown; `--out <path>` writes a file; `--json`/`--toon` emit structured data. Opt-in `--llm` prose layer deferred.
- `repoctx export` (unreleased, issue #16) — self-contained interactive HTML graph (no CDN/build/server): call graph embedded as JSON + a hand-rolled vanilla-JS force layout. Only real subsystems (clusters ≥ `analysis.subsystem_min_size`, same count as `report`) get distinct colors; tiny-cluster tail + ambiguous/unclustered layer render grey. Edges styled by `ambiguous` status (dashed amber = uncertain — the differentiator). Layer toggle hides the ambiguous layer; honest dual-count subtitle. Drag/zoom/pan, search, legend. `--out <path>` writes the file. Template in `crates/repoctx/src/export_template.html` (`include_str!`).

- `repoctx prime` (unreleased, issue #11) — compact ~600-token session-start orientation digest (headline + top subsystems + hubs + entry points + skill pointer), deterministic markdown. `repoctx init` (for Claude) registers it as a **SessionStart** hook so it primes the agent's context to use repoctx over grep/cat (adoption-via-priming, decision 2026-06-16). Never cold-indexes (nudge if unindexed); graph referenced by command, not inlined.

> **Shared subsystem definition + deterministic Louvain.** `communities`/`report`/`export` all define a subsystem as a Louvain cluster with ≥ `analysis.subsystem_min_size` members (default 5, configurable) and report the **same** count. The Louvain partition is deterministic (sorted adjacency + lowest-id tie-break) — previously HashMap-order nondeterminism made the three disagree run-to-run.
- `repoctx import-cycles` / `modules` (unreleased, epic #4) — petgraph over the import graph: circular-import detection (SCC) + resolved import topology with dependency-first build order (toposort). Relative specifiers resolved to files; alias/package edges counted external. First petgraph adopter (ephemeral graph, decision 2026-06-16).
- `repoctx deps <file>` / `rdeps <module>` / `boundary --from --to` (unreleased, epic #4) — import / dependency graph for the core-8 langs; ADR-0011; schema v5 `imports` table. `deps` lists a file's import specifiers; `rdeps` finds importers by substring; `boundary` lists crossings where files under `--from` import `--to` ("does layer A import B?"), `--forbid` makes it a CI gate. String-based, query-time resolution. Remaining #4 children: import-cycle detection, module map + public API surface.
- `repoctx outline <file>` — document symbols for one file. Indented containment tree (human) or flat `{count, items}` (machine). Path arg accepts repo-relative or absolute; normalized through `to_db_path`. File-not-in-index → exit 1 with a prescriptive error.
- `repoctx definition <name>` — exact-name (case-sensitive) lookup over the workspace, kind-whitelisted to `{function, method, class, interface, type, module, macro, constant}`. `--lang`, `--limit` (default 50). Zero hits = exit 0, `count: 0`.
- `repoctx context <symbol>` — exact-name lookup (any kind) + the source window around each hit (`--context` lines either side, default 5; `--limit` matches, default 3). Reads source from disk and sets `stale: true` when the file's current `(mtime_ns, size)` differs from the indexed tuple. File deleted since indexing: warn and skip. Human mode prints a numbered listing per match; machine mode emits `{symbol, kind, location, before, body, after, stale}` rows.
- `repoctx status` — files, symbols, per-language counts, db size, schema version, staleness `{changed, new, deleted}` from a stat-walk; `--fast` omits staleness.
- `repoctx gain` / `gain top` — token-savings analytics. Records every read command except `index`/`gain`; aggregates only; `--since`, `--all`, `--history` window controls.
- Three output formats over one set of typed records (ADR-0008): human (TTY default), TOON (non-TTY default), JSON (`--json`). `--json` / `--toon` clap-mutually-exclusive. Default format also configurable via `output.default` in the per-repo settings table.
- `repoctx config show/get/set/unset` — per-repo settings (`gain.no_record`, `gain.record_query`, `output.default`, `index.nested_keys`, `analysis.subsystem_min_size`). Stored in the `.repoctx/index.db` settings table (schema v4). Precedence: CLI flag → env var → settings → default. Legacy `hook.*` rows are ignored silently.
- `repoctx init [-g]` — the single onboarding command. Installs the per-agent guidance files (SKILL.md + CLAUDE.md/AGENTS.md block) and, for Claude, adds a **SessionStart** hook entry to `.claude/settings.json` that runs `repoctx prime` so the digest is injected at session start. `--agent <name>` (claude/codex/opencode; codex/opencode are guidance-only), `--yes`, `--force`, `--dry-run`. `--uninstall` reverses it. Embedded, offline, version-locked. Adoption is via priming, not command interception.
- No missing-index error surface for users — read commands always build the DB if needed and incrementally reindex changed files before answering.
- `repoctx languages` — surfaces the per-language coverage matrix; read commands attach an `advisory` field to machine output when the query underperforms because of language coverage limits. Agents fall back to `rg` when present.
- Languages with full coverage: Go, Rust, TypeScript + TSX, JavaScript, Python, Markdown, and the v0.7.0 batch — Ruby, C, C++, Java, C#, PHP, Lua, Kotlin, Swift (upstream `tags.scm` where shipped; vendored minimal queries for Kotlin; Swift captures struct/func/method but not class names).
- Languages with partial coverage: JSON / YAML / TOML (top-level keys by default; `index.nested_keys = true` opts into all-depth key extraction), and Bash (function definitions only). The advisory layer warns + suggests `rg` for exhaustive search.

## Releases + CI

- **CI** (`.github/workflows/ci.yml`) — `fmt --check`, `build`, `test`, `clippy -D warnings`, `platform-check`. Three-OS matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`). Runs on every push to `main` + every PR.
- **Release** (`.github/workflows/release.yml`) — triggers on `v*` tag push. Matrix builds `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.

## Test coverage

~28 workspace test suites green (live total: `cargo test 2>&1 | grep -c "test result: ok"`). Notable areas:

- **Onboarding**: `init` (guidance install / global / dry-run / `--uninstall`) and the SessionStart hook wiring (`session_hook` settings.json entry add/remove).
- **Correctness suite** (CI-gated): accuracy parity vs ripgrep across 10 language fixtures with a known-symbol sidecar.
- Config round-trip/precedence, advisory generation, output format snapshots, TS/TSX vendored tags regression, `prime` digest generation.

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
- **Import / dependency graph** (`#4`, epic): `deps`/`rdeps`/`boundary`/`import-cycles`/`modules` landed (schema v5, core-8 import extraction, ADR-0011; cycles + topology via petgraph, decision 2026-06-16); remaining child — public API surface (exported symbols per module; needs `pub`/`export` extraction).

## Broken / regressions

None known.

## Not started

- Long-lived daemon + LSP backend — placeholder epic. **Do not pre-empt.**
- Linux aarch64 / linux-musl release artifacts (current release workflow ships x86_64-gnu only on Linux).
- crates.io publish (deferred until API stabilizes; track CHANGELOG).

See `gh issue list` filtered by the `milestone:*` label.
