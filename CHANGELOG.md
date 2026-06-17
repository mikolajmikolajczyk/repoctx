# Changelog

All notable changes to this project will be documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Complete, grep-free discovery path from session start.** `prime`'s cheat-sheet is now an explicit **intent→command** map (find a symbol → `search`, who-calls → `callers`/`callgraph`, architecture → `overview`/`report`/`communities`, drill into a subsystem → `callgraph <label> --direction both`, …), with a hard "navigate with repoctx, NOT grep/cat/find" directive, a fallback rule (only grep on partial-coverage languages or for prose reasoning after locating code), and a pointer to the full skill reference. The agent gets the repo's shape **and** the exact command for any structural question in one session-start payload — no static per-subsystem files needed (those would go stale; the live commands are fresh + exactly-scoped).
- **Refreshed the `repoctx` skill (`SKILL.md`).** Removed the obsolete "transparent rewrite / hook" section (the hook is gone) and replaced it with an "use repoctx instead of grep/find/cat" intent table; added the orientation commands (`communities`/`report`/`export`) and a subsystem drill-down note. The skill is now current + complete.

### Changed

- **SessionStart priming now goes through an editable script.** Instead of wiring `repoctx prime` straight into `settings.json`, `repoctx init` writes a bashrc-style `.claude/hooks/session-start.sh` and points the SessionStart hook at it (`bash .claude/hooks/session-start.sh`). The script has a **managed block** (`repoctx prime`, regenerated on re-`init`) plus a **user region** below it that's preserved across re-runs — so you can append your own session-start context (anything echoed to stdout lands in the agent's context). `--uninstall` strips the managed block and removes the script only if you never added anything; otherwise it keeps your lines. `-g` writes `~/.claude/hooks/session-start.sh`.

## [0.13.0] — 2026-06-17

### Removed

- **The per-command PreToolUse rewrite hook is gone — repoctx no longer intercepts `grep`/`rg`/`find` (BREAKING).** Telemetry showed it converted ~0% of real agent traffic (multi-term/path/find blocked by quoted metachars + `cd`-prefix), and most of that traffic was already compressed by the chained `rtk` proxy — so the brittle shell-rewrite machinery wasn't earning its keep. Removed: `repoctx hook` (all subcommands), `repoctx rewrite`, `repoctx discover` + its `hook_events`/`hook_samples` telemetry, the generated `.repoctx/hook.sh` script + settings takeover/doctor/drift logic, rtk chaining, and the `hook.*` config keys (`hook.rewrite`/`use_rtk`/`chainable`/`chain_commands`/`telemetry`/`telemetry_samples`). Old `hook.*` settings rows are now ignored silently; `config set hook.*` reports them obsolete. Adoption is now **session-start priming** (below). ~2,800 LOC deleted. Migration: re-run `repoctx init` to switch from the old PreToolUse hook to the SessionStart prime hook; remove any leftover `~/.claude/repoctx-hook.sh` + its PreToolUse entry by hand if present. (If you relied on the hook to chain `rtk`, install rtk's own hook directly — repoctx no longer manages it.)

### Added

- **`repoctx prime` + SessionStart priming — adoption via context, not interception (issue #11).** A new compact, token-budgeted (~600 token) repo orientation digest — headline (files/code symbols/languages), top subsystems (#14), hubs, entry points, and a `repoctx` command cheat-sheet — generated deterministically from the index. `repoctx init` registers it as a **SessionStart** hook, so the digest lands in the agent's context at session start: the agent begins with a structural map and a nudge to use `repoctx search/outline/callers/...` instead of blind `grep`/`cat`. `prime` never cold-indexes (it emits a one-line nudge if unindexed, so session start stays fast) and refreshes incrementally otherwise; the full call graph is referenced by command (`repoctx export`), never inlined. Decision: `wiki/decisions/2026-06-16-adoption-via-priming.md`.
- **`repoctx init` is now the single onboarding command.** Installs agent guidance files and (for Claude) wires the SessionStart prime hook; `--uninstall` removes it; `-g` does it user-global. Replaces the former `repoctx hook install`.

## [0.12.1] — 2026-06-16

### Fixed

- **`communities`/`report`/`export` now agree on the subsystem count.** Two root causes: (1) **Louvain was nondeterministic** — adjacency was built from a randomized `HashMap` and the local-moving phase iterated a `HashMap`, so each invocation (they're separate processes) produced a slightly different partition (e.g. 25 vs 23 vs 23). Adjacency construction and candidate-community selection are now deterministically ordered (sorted, lowest-id tie-break), so the partition is reproducible. (2) **No shared definition of "subsystem"** — `report` capped at 12, `communities` at 30, `export` colored all 114 raw clusters. A subsystem is now a Louvain cluster with **≥ `analysis.subsystem_min_size` members** (default 5, configurable), and all three report that same count; display caps (how many to list) are separate from the count.

### Added

- **`analysis.subsystem_min_size` config** (default 5; `REPOCTX_ANALYSIS_SUBSYSTEM_MIN_SIZE`). The shared minimum cluster size that counts as a subsystem across `communities`/`report`/`export`.
- **`export` readability + honesty.** Only real subsystems get distinct colors; the tiny-cluster tail and the ambiguous/unclustered layer render **grey**, so the eye reads subsystems as colored islands. A **layer toggle** hides the ambiguous/unclustered layer for a clean subsystem view (vs. the full graph-with-uncertainty). The subtitle now reports both sides honestly: `N subsystems · S symbols (R resolved + A ambiguous/builtin) · E edges (Re resolved + Ae ambiguous)`.

## [0.12.0] — 2026-06-16

### Fixed

- **`overview` ranks modules by code, not doc/config symbols (issue #9-D).** Module sizes counted every symbol equally, so markdown headings (`section`) and config keys (`key`) pushed `wiki/`, `.github/`, and `docs/` to the top of the module ranking. Modules now rank by **code symbols** (excluding `section`/`key`); the header reports the split (`N code symbols (T total, D doc/config)`) and each module shows its code count. On madside the top modules are now `src/ports`/`src/app`/`src/services`, with 760 doc/config symbols separated from 1175 code. `symbol_counts_by_file` returns `(path, total, code)`; `Overview`/`ModuleStat` gain a `code_symbols` field.
- **Receiver-aware call resolution — `obj.foo()` no longer binds to a free `function foo` (issue #9).** Call sites now record whether they carry a receiver value (`obj.foo()`, a method call) vs a free/path call (`foo()`, `Type::foo()`) — detected at extraction from the Tree-sitter node shape across all core-8 languages (schema v9, new `calls.is_method` column; run `repoctx index --force` to repopulate). A method call resolves **only** to a `method` of that name, never a free function, so `map.set()` stops binding to a lone `fn set` and fabricating a super-hub. This replaces the blanket `HOST_METHOD_NAMES` stop-list with a precise rule — free calls to real functions named `create`/`join`/etc. now resolve correctly (the old list dropped them everywhere). A residual guard keeps builtin **method** names (`push`/`get`/`set`/…) from binding to a same-named repo *method* (a method→method collision that needs receiver *types*, deferred). Applies to `callers`/`callees`/`callgraph`/`cycles`/`hotspots`/`communities`/`report`/`export`. On madside the fake `set` (44) and `push` (28) hubs are gone; god nodes are all real functions.
- **Graph node identity: per-definition, not per-name — fixes god nodes / communities / report / export at once.** The graph (degree, clustering) keyed nodes by bare symbol name, so distinct same-named definitions collapsed into one node whose degree was the *sum* of unrelated definitions — fabricating fake super-hubs and grab-bag clusters. Node identity is now `(name, file, line)`: `set@event-bus.ts:23` and `set@storage.ts:281` are distinct nodes, and labels qualify (`name@file:line`) only when a name has multiple definitions (unique names like `getDB` stay bare). Independently, **receiver-blind host/builtin method names** (`get`/`set`/`push`/`join`/…) — which bind to a lone same-named def under name-based resolution (ADR-0010) and dominate as popularity, not centrality — are now excluded from degree/clustering on both endpoints, matching the existing `overview` hotspots heuristic (shared `HOST_METHOD_NAMES`). On madside the fake `set` super-hub (degree 44, sole origin of all 12 bridges) is gone; god nodes are now real (`App`, `getDB`, `createWorkbench`), clusters coherent, and suggested questions diversified. God-node degree + clustering use resolved (unambiguous) edges only, extending the ADR-0010 resolved-only rule.
- **`overview`/`report` entry-point detection recognizes JS/TS web-app bootstraps.** Entry points were `name = 'main'` functions only, so SPA codebases (no `main()` function) reported "none detected" despite an obvious `src/main.tsx`. Bundler-entry basenames (`main.{ts,tsx,js,jsx}`, `index.{tsx,jsx}`) are now surfaced as `kind: "entry"` records. madside's `report` now lists `main.tsx`.

### Added

- **`repoctx export` — self-contained interactive graph HTML (issue #16).** Emits ONE HTML file rendering the call graph — **no CDN, no build step, no server**: the graph is embedded as JSON and laid out by a tiny hand-rolled force simulation in vanilla JS. Nodes are colored by community (#14) and sized by degree; **edges are styled by `ambiguous` status** (the differentiator — repoctx knows which edges are name-uncertain, so the viz *shows* it: dashed amber = ambiguous, solid green = resolved). Click-drag nodes, scroll to zoom, drag background to pan, search by symbol, toggle subsystems via the legend. `--out <path>` writes the file (e.g. `graph.html`); otherwise the HTML goes to stdout. The "wow" artifact — a clickable architecture graph beats a hotspots table.
- **`repoctx report` — deterministic architecture report (issue #15).** Composes the resolved call graph into a one-page markdown summary, **generated entirely from topology — no LLM, no network**: god nodes, subsystems (#14 Louvain clusters), cross-cluster bridges (call edges whose endpoints sit in different subsystems, ranked by combined degree — the coupling worth scrutinizing), entry points, and templated "suggested questions" derived from structure (orientation prompts, not findings). Human render *is* the markdown; `--out <path>` writes it to a file (e.g. `REPORT.md`) regardless of `--json`/`--toon`; JSON/TOON emit the structured data. Preserves repoctx's cheap-and-deterministic identity; an opt-in `--llm` prose layer is deferred.
- **`repoctx communities` — subsystem clustering + god nodes (issue #14).** Runs single-level Louvain modularity optimization over the **resolved** call graph (unambiguous, single-callable-def callees only) to group symbols into subsystems, labels each cluster by its highest-degree member, and surfaces god nodes (highest-degree symbols overall). Pure topology — no embeddings, no LLM. Clustering over ambiguous fan-out produces garbage, so the input is resolved-only by construction. Output capped (30 clusters, 15 members each, 15 god nodes) for token thrift; JSON/TOON/human. The "dropped into an unfamiliar repo, where are the seams" command.

## [0.11.10] — 2026-06-16

### Added

- **tsconfig path-alias resolution for the import graph (issue #8).** A shared resolver now turns alias specifiers (`@adapters/*` → `src/adapters/*`) into indexed files, not just relative `./`/`../` imports — aliases are collected from every `tsconfig*.json` / `jsconfig.json` at the repo root (covers split base/app configs + `extends` without chasing the chain; JSONC-tolerant). **`modules`/`import-cycles`** now resolve far more of the graph (madside: external edges 393 → 163, resolved edges 286 → 501). **`boundary`** is alias-aware: it resolves each import and counts a crossing when the resolved target hits `--to`, so `boundary --from src/ui --to src/adapters` catches `@adapters/*` imports — and `count: 0` now reports how many bare/unresolved imports (node_modules/unmapped) remained, instead of a misleading "clean." Bare/package specifiers + non-TS module syntax (Rust/Python/Go) stay external (future work).

### Fixed

- **Call edges no longer resolve to data/doc symbols (issue #9).** A callee whose only same-named symbol is a JSON/YAML/TOML `key` or a markdown `section` now resolves to external (`null`) instead of binding to that data symbol — `callgraph`/`impact`/`cycles`/`callers`/`callees` stop reporting calls to e.g. a `toolchain` key in `project.json`. (The v0.11.9 fix covered only the `overview` hotspot path; this covers the shared `call_edges` join.)
- **`boundary` no longer reports a false "clean" (issue #13).** `boundary` resolves relative imports only, so alias/bare imports (`@adapters/…`) were invisible — `count: 0` looked like a passing layering check when 100+ edges weren't examined. When there are no relative crossings, the advisory now states how many alias/bare imports from the `--from` layer were **not** checked and how to query them ("NOT a clean bill"). Real alias resolution is tracked in #8.

## [0.11.9] — 2026-06-16

### Changed

- **Call-graph aggregation now consumes edge quality (issue #9).** `callers`/`callees`/`callgraph`/`impact` gain `--resolved-only` (drop ambiguous + external edges, keep only names resolving to a single in-repo symbol); without it, resolved edges sort first. **`overview` hotspots** are de-noised: count only names resolving to a single callable definition, exclude a stop-list of host/builtin method names (`get`/`set`/`push`/`has`/`map`/… — `.get()`→Map, not your symbol), and resolve representative locations to callable kinds only (so `.on()` no longer binds to a YAML `key` named `on`). On a real TS repo, hotspots flip from `get`/`set`/`push` collisions to genuine central functions (`getDB`, …). Remaining: deeper receiver-awareness (`foo.push()` vs `push()`) instead of a stop-list.

## [0.11.8] — 2026-06-16

### Added

- **Lexical visibility extraction — Go + JS/TS (issue #10).** New `visibility` column on symbols (`public`/`private`/`unknown`), set syntactically by the extractor. **Go**: exported iff the identifier's first letter is uppercase. **JS/TS/TSX**: exported iff the definition is wrapped by an inline `export` (`export function/class/const`, `export default`) — cuts the common `create*`-factory false positives. `deadcode` now excludes `public` symbols — exported API called across boundaries (or via DI) is no longer falsely flagged dead. `unknown` is the safe default for languages without extraction yet (no behavior change). Schema v8; run `repoctx index --force` to populate visibility for existing rows. Not yet: `export {{ … }}` clauses / CJS (those stay `private` for now), Rust `pub`. Also unlocks the future `overview` public-API surface.

## [0.11.7] — 2026-06-16

### Changed

- **Skip minified/generated files at index time** (issue #9): files with any line longer than 5000 chars (bundles, emscripten glue, source maps) are no longer indexed — they flooded `symbols`/`overview` hotspots/`deadcode` with machine-emitted noise. A file that *becomes* minified-skipped (or was indexed by an older build) has its stale symbols purged on the next index, not orphaned. Cuts noise across every command.
- **`deadcode` tightened** (issue #9): now restricted to the core-8 call-graph languages (in any other language there are no call edges, so *every* function looked dead — e.g. every Bash function); excludes `constructor` (invoked via `new`, never called by name), test files (`.test.`/`.spec.`/`_test.`/`tests/`/`test/`), `.d.ts` declarations, and (via the above) minified files. Dynamic-dispatch / public-API false positives remain (name-based, ADR-0010); advisory says so. (Validated on a real repo: ~44% fewer candidates.)

## [0.11.6] — 2026-06-16

### Added

- **`repoctx changed [--since REF]`** (issue #6): change-aware blast radius for code review. Diffs the working tree against a git ref (default `HEAD`), maps changed lines to the **symbols** they overlap, then walks their **transitive callers** = "what this change touches + what it can break." Output: changed symbols + impacted callers tagged with BFS depth. Reuses the call graph's narrow `callers` queries (reverse BFS, name-based per ADR-0010 — dynamic dispatch / external callers invisible; advisory says so). Tracked files only (untracked aren't in `git diff`); blast radius capped at 500.
- **`repoctx overview`** (issue #5): repo architecture in one call — totals, per-language breakdown, per-directory module sizes (files/symbols/bytes, ranked by symbols), entry points (`main` functions), and hotspots (most-called symbols from the call graph). The "agent dropped into an unfamiliar repo" command; replaces dozens of `ls`/`cat`/grep round-trips. Composes data the index + call graph already hold — no new extraction. Public API surface (exported symbols per module) is intentionally absent until per-language export extraction lands (#8); the advisory says so. Hotspots are name-based (ADR-0010).

### Changed

- **Agent guidance refreshed** to the full v0.11.x command surface — SKILL.md + the AGENTS.md / CLAUDE.md fragments now document/cue every nav command (deadcode/impact/cycles, deps/rdeps/boundary, import-cycles/modules, overview, changed).

## [0.11.5] — 2026-06-16

### Added

- **`repoctx import-cycles` / `modules`** (epic #4, ADR-0011): graph analyses over the import graph. `import-cycles` finds circular imports (petgraph `tarjan_scc`); `modules` emits the resolved import topology + a dependency-first build order (petgraph `toposort`), flagging cyclic graphs. To get file→file edges, relative specifiers (`./x`, `../y`) are resolved against the indexed file set (common extensions + `/index`); alias/package specifiers (`@scope/x`, `react`) need build-config resolution we don't do yet, so they're counted as external and excluded (advisory says so). Best fit for JS/TS/relative includes. **First petgraph adoption** — built as an ephemeral graph from a store query, run, dropped; SQLite stays source of truth (decision: `wiki/decisions/2026-06-16-petgraph-for-graph-algos.md`).

## [0.11.4] — 2026-06-16

### Added

- **`repoctx deadcode` / `impact` / `cycles`** (issue #3 — Tier-1 call-graph analyses, no new indexing): `deadcode` lists function/method symbols with zero incoming call edges (entry points like `main` excluded; `--lang`/`--limit`) — something grep fundamentally can't do; `impact <name>` is the blast radius (everything that transitively calls `name`, framing `callgraph --direction up`); `cycles` detects recursion / mutual recursion in the call graph (capped, large graphs skipped). All name-based (ADR-0010 accuracy class) and advisory — dynamic dispatch, traits, FFI, and out-of-scope callers are invisible, so results are candidates to verify, not proof. Pure queries over the existing v4 `calls` table.

## [0.11.3] — 2026-06-15

### Added

- **`repoctx boundary --from <path> --to <module>`** (epic #4 boundary child, ADR-0011): list files whose path contains `--from` that import a specifier containing `--to` — "does layer A import layer B?" answered from the import graph instead of regex over import lines + eslint-boundary comments. `--forbid` turns it into a CI gate (exit 1 if any crossing). Validated by telemetry: boundary/import audits were the clearest recurring structural intent behind agents' `rg @(core|ports|adapters)/…` greps. Reuses the v5 `imports` table — no schema change.

### Changed

- **`discover` classifier v2: quote-aware splitting + `pipe-filter` bucket + value-flag parsing** (issue #7 follow-up). Real captured commands exposed two bugs: (1) compound/quoted commands were split on `|`/`;` *inside quoted patterns*, so a multi-alternation regex audit like `grep -nE "a|b|c" file` mis-bucketed as `explicit-path`; segmentation is now quote-aware. (2) Greps that filter command *output* (`npx vitest | grep -iE "PASS|FAIL"`) were counted as code searches; a grep fed by a real pipe now buckets as `pipe-filter` and is excluded as a rewrite target (repoctx can't replace output filtering). `||`/`&&` are treated as control ops, not pipes. (3) Value-taking flags in separate-arg form (`-g GLOB`, `-A 3`, `-m N`, `--include X`, `-e/-f PATTERN`) had their value mistaken for the search pattern; they now consume it (`-e/-f` value IS the pattern), so `rg -g 'src/**' 'a|b'` correctly buckets `multi-term`.

## [0.11.2] — 2026-06-15

### Added

- **`repoctx discover --samples` — opt-in local command capture** (issue #7 follow-up). With `hook.telemetry_samples = true` (default **off**, local-only), the hook also stores the command **body** per idiom (capped at 20/idiom, truncated to 500 chars) so you can see what's hiding in `other`/`regex` and design rewrite rules from real commands. `repoctx discover --samples [--idiom <bucket>]` lists them. Schema v7 `hook_samples` table. Unlike the aggregate `hook_events`, these rows do hold command text — hence opt-in and local.

## [0.11.1] — 2026-06-15

### Changed

- **`repoctx discover` classifier: new `literal-string` idiom** (issue #7 follow-up). Literal, single-token, non-regex patterns that aren't bare identifiers — kebab-case, scoped packages (`@scope/pkg`), `a:b` — previously fell into the uninformative `other` bucket. They're now `literal-string`, a rewrite candidate (`rg foo-bar` → `repoctx search foo-bar`). Common in TS/CSS codebases; surfaces a real rewrite opportunity instead of hiding it. `other` now holds only multi-word / empty patterns.

## [0.11.0] — 2026-06-15

### Added

- **Hook passthrough telemetry + `repoctx discover`** (issue #7). The PreToolUse hook now records every `grep`/`rg`/`find` command it sees, bucketed by **idiom** (`bare-ident` / `flagged-nav-ident` / `regex` / `call-shape` / `import-shape` / `multi-term` / `explicit-path` / `find` / `other`) and **outcome** (`rewritten` / `passthrough` / `chained`). Aggregate-only — **no command body, no pattern, no paths** (same privacy posture as `gain`). `repoctx discover` reports per-idiom rewritten-vs-passthrough counts ranked by volume, surfacing the biggest adoption gaps — the data that drives which grep idioms to teach the hook to rewrite next. Best-effort recording: never blocks/fails a command, only writes when an index DB already exists (won't create `.repoctx/` in non-repoctx repos). Opt out with `hook.telemetry = false` (config or `REPOCTX_HOOK_TELEMETRY=0`).

### Changed

- **Schema bumped to v6** (adds the `hook_events` table). Older DBs migrate transparently on open.

## [0.10.0] — 2026-06-15

### Added

- **Import / dependency graph** (epic #4, ADR-0011): new `repoctx deps <file>` (modules a file imports) and `repoctx rdeps <module>` (files that import a specifier — substring match, so `rdeps storage-idb` finds importers of `@adapters/storage-idb`). Import sites are extracted from Tree-sitter for the core 8 languages (Rust `use`/`extern crate`, Python `import`/`from`, JS/TS/TSX ESM `import`/`export … from`, Go imports, C/C++ `#include`, Java `import`). Schema v5 `imports` table; string-based (raw specifier stored, quotes/brackets stripped), edges cascade with the file and resolve at query time — precise specifier→file resolution is deferred to a future resolver writing `semantic` rows into the same table. Answers boundary/layering questions structurally instead of grepping import lines + eslint-boundary comments. JSON/TOON/human output, gain-recorded; empty results carry an advisory.

### Changed

- **Schema bumped to v5** (adds the `imports` table). Older DBs migrate transparently on open.

> Note: 0.10.0 was not cut as a standalone tagged binary; `deps`/`rdeps` first ship in the v0.11.0 release artifact.

## [0.9.1] — 2026-06-15

### Changed

- **`repoctx init` (project) no longer refuses when a user-global repoctx hook exists** — it installs guidance only. A global hook already fires for every project, so a project-local hook would just double-fire; previously `init` aborted entirely, leaving the repo without the skill + `CLAUDE.md` guidance. Now it writes the guidance files (which never race) and skips the redundant project hook, printing a `guidance-only` note. `--force` still installs a full project hook (accepting the double-fire). The global-rtk and foreign-hook race cases are unchanged.

## [0.9.0] — 2026-06-15

### Changed

- **`repoctx search` callees grouped by index-scope resolution** (issue `cd2680f`). Replaces the per-edge `resolved`/`unresolved`/`ambiguous` tags with `internal` (one indexed def — expanded with location), `ambiguous` (several indexed defs — collapsed to a per-name count), and `external_count` (calls whose definition isn't in the indexed scope — stdlib/third-party/builtin/uncovered-language — collapsed to a count). Default output is signal-dense: in-codebase callees expanded, stdlib noise summarized in one line. `--all-callees` expands the collapsed `external` names + ambiguous `candidates`. No stop-list — external-ness is index absence, defined by what we parsed (not the repo boundary), so it stays truthful for uncovered files and future workspace indexing. Same grouping applied to `callers`.
- **`repoctx search` now tags every result with provenance and surfaces call edges** (issue `52a1e2c`). Output is one flat `results` stream where each item carries a `source`: `structural` (tree-sitter-confirmed symbol — name/kind/range known), `reference` (a call site of the queried name), or `textual` (unconfirmed substring — comment/string/other). Each structural symbol carries its own `callers` and `callees` (queried by that symbol's name), so the agent learns who-calls-what in one query — the thing grep can't do. Replaces the previous `{symbols, matches}` shape; lines are 0-based in machine output. (Breaking change to `search`'s JSON, which shipped in v0.8.0.)

## [0.8.0] — 2026-06-15

### Added

- **`repoctx search` — textually-complete search** (epic `f4cb992`). Returns the symbol definitions named the pattern **plus** every textual occurrence ripgrep finds (comments, strings, anything `symbols` would miss), compressed to `file:line` (caps: 40 files, 8 matches/file, 200-char lines; truncation flagged). repoctx runs real ripgrep under the hood and owns the compression. The hook now rewrites ambiguous searches (`rg foo`, `rg -n/-l/-i/-w/-F`, `rg --type L`, `grep -r foo .`) to `repoctx search` instead of the lossy `repoctx symbols` — no more silently-dropped textual matches. Structural intents still map to `definition`/`context`; `repoctx symbols` remains an explicit command. Falls back to symbol-only + advisory if ripgrep isn't installed.
- **Flag-aware hook rewrite.** The transparent `rg`→`repoctx` rewrite now understands navigation flags on a bare identifier, so agents' habitual flagged searches reach repoctx instead of bypassing to ripgrep: `rg -n/-l/-i/-w/-F <ident>` → `repoctx search` (see above), `rg --type <lang> <ident>` → `repoctx search --lang <lang>`, `rg -A/-B/-C <n> <ident>` → `repoctx context --context <n>`. Flags that change the result set rather than its formatting (`-c`, `-v`, `-o`, `--json`, unknown `--type`), regex, paths, and quoted literals still pass through. Rewriting also sidesteps the rtk-chain bypass entirely (repoctx serves it directly).
- **Static call graph** (epic `af42572`, ADR-0010): new `callers <name>`, `callees <name>`, and `callgraph <name> --depth N --direction up|down|both` commands. Call sites are extracted from Tree-sitter syntax for the core 8 languages (Rust, Python, JavaScript, TypeScript, Go, C, C++, Java) and callees resolved by name — the same accuracy class as `definition` (approximate: no receiver-type disambiguation, dynamic dispatch invisible; ambiguous/unresolved edges flagged and advised). Schema v4 `calls` table; edges resolve at query time so they survive incremental reindex, and a future LSP backend can write precise `semantic` edges into the same table. JSON/TOON/human output, gain-recorded.
- **rtk fidelity canary** (`scripts/rtk-fidelity/`, manual / never-CI). Drives probe commands through the real `repoctx hook claude` path, classifies each as bypass / semantic-rewrite / chain, and for chained commands compares rtk output against the real tool — hard-failing the silent false-empty class (what broke `ls` in rtk ≤0.41). Run it on rtk version bumps to catch new chain regressions the `is_chain_unsafe` denylist can't predict.

### Changed

- **Hook chain-bypass refactored into a single `is_chain_unsafe` guard.** Generalizes the flagged-`rg` bypass so any command the rtk chain corrupts is a one-line add, backed by a fidelity audit re-run on rtk version bumps. `ls` was briefly bypassed (rtk ≤0.41 returned `(empty)` for any directory) but rtk 0.42.4 fixed its `ls` proxy, so `ls` chains again; flagged `rg` (`-i`/`--type`/`-g`, any pipeline segment) stays bypassed — still broken as of rtk 0.42.4.

## [0.7.1] — 2026-06-14

### Added

- **Agent benchmark harness** (epic `b20a3c9`, manual / never-CI) under `scripts/agent-bench/`: bats suites that gate token savings on three SHA-pinned real codebases (helix, vuejs/core, rust-analyzer), plus `report.sh` for the per-query number table. Metric is `bytes/4`, method-consistent with `repoctx gain` (no model-specific tokenizer). Design + thresholds: `wiki/decisions/2026-06-13-agent-bench.md`.
- **Benchmark results page** (`wiki/bench/results.md`) + **[why repoctx saves tokens](wiki/user/why-repoctx.md)**. v0.7.0 baseline: **~99% token savings vs ripgrep-worst** on `definition` / `symbols` / `outline` across all three repos; `context` 65–99% (it returns the actual source window). README links both.

### Fixed

- **Flagged `rg` no longer degraded by the rtk chain.** When the hook can't structurally rewrite an `rg` command and it carries any flag (`-i`, `--type`, `-g`, …), it now bypasses the rtk chain entirely so the agent's real ripgrep runs. Previously these were handed to rtk's `grep` wrapper, which forwards unrecognized flags to GNU grep — silently losing ripgrep's recursive/gitignore defaults (empty results) and erroring on rg-only flags. Plain `rg PATTERN` and bare-identifier rewrites are unchanged. Detection now scans every pipeline/list segment, so flagged `rg` in a compound command (`cat f | rg -i x`, `cmd && rg --type rust x`) is caught too, not just a leading `rg`.

## [0.7.0] — 2026-06-13

Language coverage expansion (epic `9cf4c18`) + opt-in nested keys + an opencode runtime plugin.

### Added

- **10 new languages** indexed: Ruby, C, C++, Java, C#, PHP, Lua, Kotlin, Swift (full coverage) and Bash (functions only — partial). 20 languages total now. Grammars are statically linked per the loading-strategy decision (`wiki/decisions/2026-06-13-grammar-loading-strategy.md`); each ships an extraction test. Binary grows ~12.3 MB → ~32 MB (under the 50 MB revisit threshold).
- **Opt-in nested-key extraction** for JSON/YAML/TOML (`index.nested_keys`, default off). When on, keys at any depth are indexed (not just top-level); flip it and `repoctx index --force` to re-parse. Issue `2c47040`.
- **opencode runtime plugin** (`44183b3`, tier-2). `repoctx hook install opencode` now also writes `.opencode/plugin/repoctx.ts`, which intercepts the agent's bash `rg`/`grep` calls and rewrites them via `repoctx rewrite` — the same decision the Claude hook makes.

### Notes

- `1a19873` (rename `SymbolKind::Key` → `TopKey`) is obsoleted by `2c47040`: nested keys make "top-level only" a configurable choice, so the kind stays `key`; the top-only default is communicated by the coverage advisory + `repoctx languages` notes.

## [0.6.1] — 2026-06-13

Internal code-quality pass from the 2026-06-12 audit (`e63eb72`). No user-facing behavior change beyond added logging.

### Changed

- Tests no longer mutate the process `HOME` env (`scan_user_global_at(path)` injection) — removes a parallel-test race.
- `index` logs when a parse result is dropped because the writer hung up, and surfaces a parser-thread panic instead of swallowing it.
- Read commands (`symbols`/`outline`/`definition`/`context`) share one `emit_and_record` tail; `hook install` takes an `InstallContext` struct; `resolve_window` lives in one place; config enum errors read uniformly (`expected one of […] (got …)`); metadata-error fallbacks are commented + observable. Snapshot tests moved into `output.rs`.

## [0.6.0] — 2026-06-13

`repoctx init` — repoctx becomes the meta-hook for Claude Code. Plus the integration content moves into the binary (no network), and a CI-gated correctness suite.

### Added

- **`repoctx init` — first-class onboarding** (epic `40c8baa`). One command wires repoctx into Claude Code: writes a committed dumb-pipe hook script (`.repoctx/hook.sh`, or `~/.claude/repoctx-hook.sh` with `-g`), points `settings.json`'s sole `PreToolUse → Bash` entry at it, writes `.gitattributes`, and installs the SKILL.md + CLAUDE.md guidance. Flags: `--agent`, `--rtk auto|on|off`, `--yes`, `--force`, `--dry-run`, `--uninstall [--restore-backup]`.
- **repoctx is now the meta-hook.** The hook script execs `repoctx hook claude`; all rewrite/JSON/chain logic lives in the binary (no `jq`). On passthrough it chains the first allowlisted tool on PATH (`hook.chainable`, default `["rtk"]`) via `--rtk-chain`, forwarding rtk's output verbatim — repoctx's structural rewrites **and** rtk's compression, no race.
- **Race detection.** `init` refuses to create a configuration that would race (a foreign hook anywhere, or a repoctx/rtk hook in a scope that double-fires with the target), with actionable resolution; `--force` overrides. `init -g` over a global rtk hook backs up `settings.json`, takes over, and chains rtk.
- **`repoctx hook doctor [-g] [--fix]`** — drift/tamper check (re-renders the expected script + structurally compares), settings-entry + foreign-hook report; `--fix` regenerates + restores with a backup.
- **`repoctx rewrite <cmd>`** — debug/bench utility exposing the hook's rewrite decision (exit 0 + rewritten command, or 1 = passthrough).
- **Config keys** `hook.use_rtk` (`auto|on|off`), `hook.chainable` (allowlist), `hook.script_path` (read-only). v0.5.x installs auto-migrate on first `init` (chain_commands → RTK_CHAIN, row dropped).

### Changed

- **Integration content is embedded in the binary** (issue `43aeaff`). `repoctx hook list/status/install` previously fetched per-agent manifests + fragments from a GitHub mirror at a pinned git ref, cached under XDG. The content was always version-locked to the binary, so the fetch bought nothing — now it's compiled in via `include_str!`. `hook install` works offline / airgapped and always matches the running binary.

### Removed (BREAKING)

- **`--ref` / `--no-cache` flags** on `hook list/status/install`, and the **`hook.ref` / `hook.no_cache` config keys** + their `REPOCTX_HOOK_REF` / `REPOCTX_HOOK_NO_CACHE` env vars. Integration content is embedded; there is no fetch ref or cache to control. Old rows in an existing settings table are ignored quietly (no warning). `REPOCTX_INTEGRATIONS_CACHE_DIR` no longer does anything.
- **`ureq` + `rustls` + `directories` dependencies** dropped from the workspace — `repoctx` no longer makes any network calls. Binary ~1.9 MB smaller (14.2 MB → 12.3 MB stripped Linux).

### Tests

- **Correctness suite** (epic `1cd1fc7`), CI-gated: a ≥100-row rewrite-decision corpus asserted through both the pure decision function and `repoctx hook claude` (`573eccc`), and accuracy parity vs ripgrep across 10 language fixtures with a known-symbol sidecar (`c23894f`). Plus an end-to-end suite that runs the rendered `hook.sh` under bash across the chain/missing-binary matrix (`0a338d7`).

## [0.5.3] — 2026-06-12

Hotfix for v0.5.2's `gain` display.

### Fixed

- **No more negative savings.** `gain top` could show rows like `context  -66  -51.2%`. Sparse-result commands (`definition`/`context` with 0–few hits) have tiny or empty candidate sets, so the structured output exceeds the `candidate_bytes/4` baseline and the raw subtraction went negative. Savings is a non-negative quantity by definition — a shared `savings_and_reduction()` now floors both savings and reduction at 0, for totals and per-command rows, human and machine output. Raw bytes stay untouched in the usage table. The deeper baseline-model undercount (these commands plausibly save real tokens but report ~0%) is tracked in issue `1a5c664`. Issue `6af231f`.

## [0.5.2] — 2026-06-12

Read-surface polish: honest gain numbers, a prettier `gain` view, case-mismatch advisories, and clean error output. No hook/install behavior change.

### Added

- **Beautiful `gain` / `gain top`.** Human output gains a header + box rule, human-size units (`29.7K`), a 24-cell efficiency meter, and a ranked per-command table with impact bars. The summary embeds the top-5 commands. Machine output (JSON/TOON) is byte-for-byte unchanged — the table rides a `#[serde(skip)]` field, so the totals-only contract holds. Issue `5dd6f41`.
- **Case-insensitive near-miss advisory on `definition`.** `definition` is exact-case; `symbols` is case-insensitive. A zero-hit lookup that has a case variant (you typed `store`, `Store` exists) now carries an `advisory` naming up to three variants and suggesting the exact casing or `repoctx symbols`. Previously this looked like "doesn't exist" — a false negative for agents following the AGENTS.md rule. Fragment + SKILL.md updated. Issue `a8489e7`.

### Changed

- **Token estimation is now method-consistent.** Both sides of the savings ratio use a `bytes / 4` heuristic. Previously the baseline used `bytes / 4` but returned-token counting used tiktoken's `cl100k_base` BPE — a mixed-unit ratio, and `cl100k` is OpenAI's tokenizer (wrong model for Claude/Codex users). Recorded `gain` percentages shift slightly; the ratio is now honest. Precise, per-model BPE counting moves to the bench suite's dedicated `tokens` helper. Issue `3a7fbc1`.
- **`repoctx` binary ~1.9 MB smaller** (16.1 MB → 14.2 MB stripped Linux) from dropping the tiktoken BPE tables.

### Fixed

- **Clean error output.** Errors print a single `error: …` line to stderr and exit 1, instead of letting anyhow's `Debug` dump a stack backtrace when `RUST_BACKTRACE` is set (common in agent/CI shells). Opt into the full chain + backtrace with `REPOCTX_BACKTRACE=1`. Issue `e925e76`.
- **Stale reindex hint removed.** `outline` on a not-indexed file no longer says "run `repoctx index`" — read commands auto-index, so the message contradicted the self-managing-index promise. It now explains why the file is not indexable (not on disk, gitignored, oversized, non-UTF-8, unsupported language). Issue `e925e76`.

### Notes

- Purely read-surface. Hook install/doctor, indexing, and the machine-output contract are untouched. Existing installs need no action.

## [0.5.1] — 2026-06-12

Real-world-setup fixes: stronger Codex stickiness + honest detection of user-global hook conflicts.

### Added

- **User-global hook conflict detection.** `hook install claude` and `hook doctor` now scan `~/.claude/settings.json` for `PreToolUse → Bash` entries from other tools (typically `rtk init -g`) and emit a stderr warning naming each conflicting command. repoctx remains strictly project-scoped — we never write to user-global config; the warning explains the Claude Code merge semantics + the three workarounds (per-project install, manual disable, or accept the race). Issue `9910aab`.

### Changed

- **`integrations/shared/AGENTS.md.fragment` rewritten** for sticker agent behavior. Drops the soft "prefer repoctx" hedging in favor of an explicit "Rules" section that anchors against drift, plus a "Quick cues" intent → command table mirroring the Claude `CLAUDE.md` fragment. Worked examples are now `Don't / Do` pairs so the model gets concrete contrast. Affects every Codex / opencode install (they share the fragment). Issue `3acc420`.

### Notes

- This release is purely additive at the binary level. Existing hook installs keep working; the warning surfaces on the next `hook install` or `hook doctor` against an installed setup.
- The user-global merge problem is a documented Claude Code design limitation; the warning lists it as such. File `/feedback` upstream if it bites you.
- v0.5.0 took ownership of project-local `.claude/settings.json` — that's still the right design. v0.5.1 just makes the broader-scope cross-contamination visible.

## [0.5.0] — 2026-06-12

Transparent rewrite hook for Claude Code. Agents stop forgetting to use `repoctx`.

### Added

- **`repoctx hook claude`** — PreToolUse hook handler. Reads Claude Code's tool-use JSON from stdin, rewrites recognized `rg`/`grep` patterns to `repoctx` equivalents, emits the standard `updatedInput` JSON, exits 0. Unmatched patterns chain through any commands saved in `hook.chain_commands` (typically `rtk hook claude` and similar), then fall through to silent passthrough (exit 1).
- **Conservative rewrite rule set**: `rg <ident>` → `repoctx symbols`, `rg "fn|class|struct|function <ident>"` → `repoctx definition`, same shape for `grep -r`/`grep -rn`. Refuses regex, multi-token patterns, shell metacharacters, paths other than `.`, and quoted single literals (explicit literal-grep intent).
- **`.claude/settings.json` ownership takeover** — `repoctx hook install claude` now displaces any pre-existing PreToolUse → Bash entries (e.g. `rtk hook claude`) into the new `hook.chain_commands` config key and installs a single `{"command": "repoctx hook claude"}` entry. Solves the parallel-execution race documented in the design doc — Claude Code 2.1.112 runs sibling PreToolUse hooks in parallel with no `updatedInput` precedence ladder, so single-entry ownership is the only reliable design.
- **`repoctx hook doctor`** — re-runs the takeover step idempotently. Run after `rtk` reinstall (or any other PreToolUse-touching installer) to recover ownership. Additive: merges newly-discovered commands into the existing chain instead of overwriting.
- **`hook.chain_commands` config key** — `\n`-separated list of hook commands the rewrite handler walks on passthrough. Surfaced in `config show / get / set` like every other config row.
- **Design doc** at `wiki/decisions/2026-06-12-rewrite-hook-design.md` carries the binding contract: rule set, safety boundaries, chain dispatch semantics, takeover algorithm, Claude Code source-confirmed parallel-execution findings.
- **`integrations/shared/SKILL.md`** grows a "Transparent rewrite (it may already be happening)" section telling agents which patterns get rewritten and how to bypass for a one-off literal-search.
- **`wiki/user/hook.md`** grows a "Transparent rewrite" section + "Coexistence with other hook installers" subsection covering the rtk install-order recommendation.

### Changed

- **`hook.rewrite` consumer is live.** The config key plumbed in v0.4.0 now drives behavior: `auto` (default) runs the design, `off` skips semantic rewrites and goes straight to chain, `force` relaxes the parser (debug-only, undocumented in user copy).
- **`integrations/shared/SKILL.md`** drops the "stop forgetting to use repoctx" implicit guidance in favor of "the rewrite hook may already be handling this for you" honest explanation.
- **`wiki/agents/architecture.md`** read-path step #6 covers the rewrite hook's runtime + install pipeline.
- **`wiki/agents/status.md`** Works section names the new `hook claude` + `hook doctor` subcommands.

### Notes

- **Codex + opencode**: their integration tiers are different (rules-only for Codex, plugin file for opencode) and don't need hook takeover — the existing `repoctx hook install <agent>` already does the full integration. A follow-up epic generalizes per-agent extensibility once we ship the next full-hook agent (likely Cursor).
- **Conflict-free with rtk** via take-ownership-at-install. If rtk is reinstalled later, run `repoctx hook doctor` to re-take ownership and pick up rtk's command into the chain.
- **No telemetry on rewrites yet** — the rewritten command's own `gain` row already captures the savings when it runs. Adding `hook-rewrite` rows is deferred.

## [0.4.0] — 2026-06-12

Per-repo config system + a rewritten README that finally explains the agent value story.

### Added

- **`repoctx config`** — new subcommand family (`show` / `get` / `set` / `unset`) over a persistent settings layer. Six initial keys: `hook.rewrite`, `hook.ref`, `hook.no_cache`, `gain.no_record`, `gain.record_query`, `output.default`. See [`wiki/user/config.md`](wiki/user/config.md).
- **Settings storage** — new `settings` key/value table inside `.repoctx/index.db`. Schema version bumps from `2` → `3`. The migration is idempotent + runs under the existing `BEGIN IMMEDIATE` guard alongside the v2 gain migration. Older binaries warn on unknown keys but don't crash, so newer-binary writes don't brick older readers.
- **Four-source precedence**: CLI flag → `REPOCTX_<SECTION>_<KEY>` env var → settings row → built-in default. `Source::{Cli, Env, Settings, Default}` tracked per field; `config show` annotates each row with its origin.
- **Persistent `output.default`** — `output::resolve` learned a third arg layering between `--json`/`--toon` flags and TTY detection. `config set output.default json` makes every read command emit JSON without `--json`. `auto` (default) preserves today's behavior (Human on TTY, TOON on pipe).
- **Persistent `gain.no_record` / `gain.record_query`** — same toggles as the existing CLI flags but reusable across invocations. CLI flag is "force on" — present flag wins over a `false` config, absent flag falls back to whatever the config layer resolved.
- **Persistent `hook.ref` / `hook.no_cache`** — defaults for the hook fetcher. Useful for teams that want every `hook install` invocation pinned to the same git ref.
- **`hook.rewrite` enum** (`auto`/`off`/`force`) plumbed end-to-end. The consumer — the transparent rewrite hook itself — lands in v0.5.0. Plumbing now means v0.4.0 users can pre-set the kill switch.
- **Legacy `RUST_REPOCTX_NO_RECORD` env var** still works as a back-compat alias for `REPOCTX_GAIN_NO_RECORD`. Documented as deprecated.

### Changed

- **README rewrite.** Drops the "Tree-sitter parses, SQLite stores" opener in favor of a concrete agent-pain narrative — `rg` returns 30 matches across 12 files, agent opens every file with `Read`, LLM pays the bill. Bench numbers from the helix codebase up front: 8,206 tokens (repoctx) vs 1,911,398 tokens (rg + every match). New "What it does" use-case bullets before any command syntax; new "How it works (short version)" demystifying the mental model without grammar / parser jargon.
- **`output::resolve` signature** — third parameter (`OutputDefault`) for the layered fallback. Tests cover all four precedence combinations.
- **`GainOpts::from_cli`** — takes the loaded `GainConfig` and OR-merges the CLI flag with it.
- **Decision doc** at `wiki/decisions/2026-06-12-config-schema.md` is the binding contract for the schema + precedence rules.

### Fixed

- The hard-coded gain `ENV_NO_RECORD` check in `gain::Recorder::record` is gone; the config layer's `apply_env` handles the env var resolution centrally. Removes a hidden behavior that bypassed the new precedence ladder.

### Notes

- Six config keys × four sources × one stored table = a small surface that grows additively. New keys are non-breaking; renamed/removed ones would be (we have none planned).
- Per-repo only. No global `~/.config/repoctx/`. Revisit only when a real cross-repo use case shows up.
- Hot reload is not implemented and won't be — each invocation reads fresh. Good enough for a CLI.

## [0.3.0] — 2026-06-12

Richer TypeScript / TSX coverage + a coverage-advisory layer so agents know when to fall back to `ripgrep`.

### Added

- **`repoctx languages`** — new subcommand surfacing the per-language coverage matrix. Returns `{slug, coverage: "full"|"partial", notes}` per supported language. Agents cache once per session to decide when to fall back to `ripgrep`.
- **Coverage advisory on every read command.** `outline`, `definition`, `context`, and `symbols` machine output now carries an optional top-level `advisory` field. Omitted in the happy path; populated when (a) the target file's language has `partial` coverage, (b) `--lang <slug>` filters to a `partial` language, or (c) `count == 0` and the workspace contains files in a `partial` language. Human render appends a final `advisory: <text>` line. The advisory always includes a concrete `rg -n <pattern>` fallback.
- **Richer TypeScript / TSX symbol coverage.** Vendored Aider's `typescript-tags.scm` (Apache-2.0) plus arrow-function patterns from Aider's `javascript-tags.scm` (same source). Plain `class`, plain `function`, arrow-function-assigned-to-`const`, `type` aliases, and `enum` declarations now surface across TS and TSX — they were all silently dropped by upstream `tree-sitter-typescript`. Empirical: a demo TSX file with React-style components went from 3/11 to 11/11 symbols captured.
- **`Language::coverage()` + `Language::notes()`** in `repoctx-index` — public surface so external callers can build their own routing logic on top of the same data the advisory layer uses.
- **`Language::from_slug()`** — inverse of `slug()`, used by the advisory layer to round-trip per-language counts from the store.
- **Coverage matrix in `wiki/user/commands.md`** with a `## repoctx languages` section and a separate "Coverage advisory on read commands" reference.

### Changed

- **TypeScript / TSX `kind` quirks table** in `wiki/user/commands.md` — drops the "plain `class`/`function` not surfaced" row (no longer true with the vendored query) and replaces it with the broader coverage explanation.
- **`integrations/shared/SKILL.md`** — new "Coverage advisory" section telling agents to check the field on every response and run the suggested `rg` command when present. The TS/TSX upstream-limitation paragraph is gone.
- **`wiki/agents/status.md`** — Languages line splits into "Full" and "Partial" blocks. Adds the `languages` subcommand + advisory mechanism to the Works section.
- **Extractor capture matcher** accepts both bare `@name` (upstream convention) and dotted `@name.definition.X` (Aider convention) so vendored queries plug in without rewriting captures. `definition_kind()` now maps `enum`, `struct`, `trait` to their typed kinds so Aider's enum captures land as `SymbolKind::Enum` instead of falling through to `other`.
- **`TreeSitterBackend::store()`** — new borrow method so command handlers can ask the store for per-language counts (needed by the advisory layer) without consuming the backend mid-pipeline.
- **`List<T>` output wrapper** — gains `advisory: Option<String>` field + `with_advisory()` builder. Skip-serialized in the happy path so machine output shape is unchanged for queries that don't trigger an advisory.

### Fixed

- **Enum capture mapping** — extractor now recognizes `@definition.enum` (previously fell through to `other`). The Aider-vendored queries depend on this; the same fix benefits any future grammar whose tags.scm uses the standard capture names.

### Notes

- The TS/TSX tags.scm vendor brings an Apache-2.0 dependency. `crates/index/queries/NOTICE` carries the attribution; LGPL-3.0-or-later compatible.
- Markdown stays at `full` coverage — heading-only is the right model for prose.
- JSON / YAML / TOML stay at `partial`. The opt-in nested-key extractor lives in issue `2c47040` (deferred to v0.6.0 — needs the config system from v0.4.0 to expose the switch cleanly).
- The `SymbolKind::Key` → `TopKey` rename (issue `1a19873`) was slated for v0.3.0 but deferred to a follow-up to keep the v0.3.0 surface focused on coverage transparency rather than a breaking-name change.

## [0.2.1] — 2026-06-11

Indexing now self-manages on every read; `--no-auto-index` removed; large docs sweep.

### Changed

- **Read commands auto-reindex before answering.** `symbols`, `outline`, `definition`, and `context` now run an incremental `index` pass before serving the answer — cheap on the no-op path (only files whose `(mtime_ns, size)` tuple changed get reparsed), quiet on stderr unless work happened. `status` and `gain` keep the lighter "build DB if missing, never reindex on top" behavior. Practical effect: `context`'s `stale: true` flag is effectively always `false` in normal use; the indexed `location` matches the on-disk source the agent reads.
- **`stale: true` documented honestly.** Previous wording suggested any read command would auto-fix staleness; in 0.2.0 only the missing-DB case triggered a reindex. The wording now matches the new behavior and points at `repoctx index` for edge cases.

### Removed

- **`--no-auto-index` global flag.** Indexing is what `repoctx` does; a flag to bypass it surfaces a "stale: true" edge case that wasn't useful in practice. Anyone who needs to assert "the index already exists, error otherwise" can `test -f .repoctx/index.db` in a script. This drops `Cli::no_auto_index` plus the `no_auto_index: bool` parameter on every read-command `run()` and removes four tests that asserted the bail path.

### Docs

- **README** — flagship `context` sample on top, three install paths (pre-built binaries with sha256 verification, `cargo install --git --tag`, `nix run` / `nix profile install`), quickstart spanning every read command, "Wire it into your coding agent" section advertising `hook install`, documentation index linking every wiki page.
- **`wiki/user/installation.md`** — rewritten. Pre-built binaries first with per-target table + curl + PowerShell snippets + sha256 step. Nix path simplified. Cargo path notes that `rusqlite` ships bundled SQLite. Verifying section names every shipped subcommand.
- **`wiki/agents/architecture.md`** — was labeled "Pre-alpha scaffold"; now reflects v0.2.x reality (current crate layout including `integrations`, command table covers every shipped command, data-flow includes hook install path and gain recording, distribution names the release workflow).
- **`wiki/agents/status.md`** — `Works` section includes hook + release CI; `Not started` now lists daemon/LSP + linux arm/musl + crates.io.
- **`wiki/agents/commands.md`** — bench + hook-e2e added to the test block; new Releasing section + Hook section for local dev with `REPOCTX_INTEGRATIONS_CACHE_DIR`.
- **`wiki/user/{commands,quickstart,gain,index,hook}.md`** — refreshed; `v0.1.0` example refs bumped to `v0.2.0`.
- **Removed milestone jargon** (`M0`/`M1`/`M1.5`/`M2`) from user-facing and agent-facing docs; the Radicle `milestone:*` labels are the source of truth. CHANGELOG and ADRs deliberately kept as-is.
- **`AGENTS.md`** — opener lists Codex and opencode alongside Claude Code; points at `wiki/user/hook.md`.

### Notes

- The `stale` field stays in the `context` machine output schema. It survives because the read-then-disk-read window isn't atomic — a file edited between the auto-reindex and the source read still flags. Effectively always `false` in normal use, but the field is the agent's safety net.
- Net change: `-149 lines` across the source tree; behavior simplification more than feature work.

## [0.2.0] — 2026-06-11

M1 navigation surface, M1.5 per-agent installer, gain wired everywhere, release-binary CI.

### Added

- **`repoctx outline <file>`** — document-symbol view of one file. Indented containment tree in human mode (stack-walk over symbols pre-ordered by `(start_line, start_column)`); flat `{count, items}` with 0-based ranges in machine mode. Path argument accepts repo-relative or absolute (canonicalized and re-anchored). File-not-in-index bails with a prescriptive error.
- **`repoctx definition <name>`** — exact-name (case-sensitive) lookup auto-filtered to the eight definition kinds (`function`, `method`, `class`, `interface`, `type`, `module`, `macro`, `constant`). Field/variable noise excluded so `definition path` doesn't drown in struct-field hits. `--lang`/`--limit` apply.
- **`repoctx context <symbol>`** — exact-name match plus a source window per hit. `--context C` lines either side (default 5), `--limit N` matches (default 3). Reads source from disk; sets `stale: true` when the file's current `(mtime_ns, size)` no longer matches the indexed tuple. Deleted-since-index files are dropped with a warn.
- **`repoctx hook list | status | install`** — per-agent install machinery for Claude Code, Codex, and opencode. Pulls manifests + files from the GitHub mirror at a pinned git ref (default `v<binary version>`), caches under XDG (override via `REPOCTX_INTEGRATIONS_CACHE_DIR`). Three modes — `write`, `append`, `merge-section`. Flags: `--dry-run`, `--force`, `--ref <git-ref>`, `--no-cache`. No `uninstall`: every install prints a per-file removal recipe.
- **Integrations content** in-tree under `integrations/`:
  - `shared/SKILL.md` — canonical Claude-skill-format guidance with frontmatter, command reference, prefer-this-over-that decision rules, kind/lang/limit semantics, upstream `tags.scm` quirks.
  - `shared/AGENTS.md.fragment` — codex + opencode AGENTS.md merge-section block.
  - `claude/CLAUDE.md.fragment` — Claude-specific CLAUDE.md guidance.
  - Per-agent manifests dispatching to the right destinations (`.claude/skills/repoctx/SKILL.md` for claude; `.agents/skills/repoctx/SKILL.md` for codex + opencode).
- **`repoctx-integrations`** workspace crate carrying the manifest schema, ureq+rustls HTTP fetcher with XDG cache layer, and installer with templated `{REPOCTX_BIN}` / `{REPO_NAME}` / `{REPO_ROOT}` variables.
- **Gain analytics now records every read command** including the M1 trio. `gain top` shows per-command savings breakdown across `symbols`, `outline`, `definition`, `context`.
- **Release-binary CI** — `.github/workflows/release.yml` builds four targets on every `v*` tag push (linux x86_64, macOS aarch64 + x86_64, windows x86_64), packages as tar.gz / zip with sha256 sidecars, attaches them to the matching GitHub release via `softprops/action-gh-release@v2`.
- **`wiki/user/hook.md`** — full reference for the install command: per-agent file table, mode reference, distribution model, template variables, removal recipe, troubleshooting.
- **`Store::file_exists` / `Store::file_stat`** — `outline` (file-in-index probe) and `context` (staleness check) ride these.

### Changed

- **`TreeSitterBackend::workspace_symbols`** — `limit = 0` now omits the LIMIT clause entirely instead of binding `usize::MAX`, which SQLite parsed past `i64::MAX` and returned "datatype mismatch". `definition` + `context` rely on the unlimited path.
- **`wiki/user/commands.md`** — three M1 sections under the M0 set; new `hook` section pointing at `hook.md`.
- **`wiki/user/quickstart.md`** — flagship `context` walkthrough now lives at step 4; gain moves to step 5.
- **`wiki/user/output-formats.md`** — CLAUDE.md recipe recommends all four read commands with "prefer this over X" guidance instead of just `symbols`.

### Fixed

- **macOS CI** — `directories` ignores `XDG_CACHE_HOME` on macOS, breaking the hook e2e suite. Added `REPOCTX_INTEGRATIONS_CACHE_DIR` env override (checked before `ProjectDirs`); tests + power users opt in via the env var, default path unchanged.
- **Windows CI** — `PathBuf::from("/etc/passwd").is_absolute()` returns false on Windows. `manifest::File::validate` now rejects `/`- or `\`-rooted dest strings explicitly so a manifest hostile on either platform fails fast.
- **Windows clippy** — `result_large_err` triggered on `IntegrationsError` because `PathBuf` is larger on Windows. Allowed at crate level with a one-line rationale; not a hot path.

### Notes

- Binary size grew ~2.5 MB (15→17.5 MB on x86_64-linux) — ureq + rustls + tokio-free TLS stack for the integrations fetcher.
- The integrations cache lives at `<XDG_CACHE_HOME>/repoctx/integrations/` on Linux, `~/Library/Caches/dev.repoctx.repoctx/integrations/` on macOS, `%LOCALAPPDATA%\repoctx\repoctx\cache\integrations\` on Windows, unless `REPOCTX_INTEGRATIONS_CACHE_DIR` is set.
- `opencode` integration assumes the `.agents/skills/<name>/SKILL.md` convention; verify against the live opencode version before relying on it.

## [0.1.0] — 2026-06-11

First tagged release. M0 functional surface complete on Linux, macOS, and Windows.

### Added

- **`repoctx index`** — incremental, mtime-based Tree-sitter indexing across 9 languages (Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown). Rayon parses in parallel, a single sequential SQLite writer persists, deleted files are pruned in one transaction. `--force` reparses everything.
- **`repoctx symbols <query>`** — case-insensitive substring search across the index with `--kind`, `--lang`, `--limit` filters and deterministic ordering. Empty result is exit 0 with `count: 0`.
- **`repoctx status`** — file/symbol counts, per-language breakdown, db size, schema version, and `(changed, new, deleted)` staleness from a stat walk. `--fast` skips the staleness walk.
- **`repoctx gain`** + **`repoctx gain top [--by saved|ratio]`** — surface the navigation tokens repoctx has avoided. Default window: last 30 days; `--since Nd|Nh|Nm|Ns` overrides; `--all` removes the window. `--history [N]` swaps the summary for the most recent rows.
- **Auto-index by default** — read commands (`symbols`, `status`, `gain`, `gain top`) auto-index when `.repoctx/index.db` is missing, printing one progress line to stderr. Scripts that want the old "bail with exit 1" can pass the new global `--no-auto-index` flag.
- **Output formats** — human (TTY default), [TOON](https://github.com/toon-format/toon) (non-TTY default), JSON (`--json`). `--json` and `--toon` are clap-mutually-exclusive. All three are encodings of the same typed records.
- **Backend abstraction** — `CodeIntelBackend` trait + `TreeSitterBackend` impl. Position-based methods (`definition`, `references`, `hover`) return a typed `Unsupported` until the M2 LSP backend lands.
- **Storage** — SQLite (`rusqlite` bundled). Schema v2: `files`, `symbols`, `meta`, `usage`. Migrations apply on open under `BEGIN IMMEDIATE` so two `repoctx index` processes on a fresh DB serialize cleanly. `busy_timeout` = 5 s; WAL on; `foreign_keys` ON.
- **Concurrency + corruption handling** — typed `Locked`/`Corrupted`/`NewerSchema` errors surface to the user with prescriptive messages and exit 1; never auto-delete the database.
- **Platform-agnostic** — no `std::os::unix` APIs; one path-helper pair (`to_db_path` / `from_db_path`) guards the fs boundary; CI on ubuntu/macos/windows + a `scripts/platform-check.sh` regression gate.
- **CI** — `.github/workflows/ci.yml` runs `fmt --check`, `build`, `test`, `clippy -D warnings`, and the platform check on every push/PR across all three desktop OSes.
- **Benchmarks** — `scripts/bench.sh` with hard budgets (cold 10 s / no-op 300 ms / symbols 100 ms / status 50 ms). Manual gate; baseline on a 5,000-file synthetic corpus: 318 / 50 / 3 / 5 ms.
- **Nix flake** — `packages.default` builds the release binary; `nix run github:mikolajmikolajczyk/repoctx` works.
- **User docs** under `wiki/user/`: install, quickstart, command reference, output formats + agent integration guide, gain analytics philosophy + privacy stance.

### Notes

- Distribution: source only. `nix run`, `nix profile install`, or `cargo install --git` are the supported paths. crates.io publishing is deferred until the API stabilizes.
- Release binary size: ~14 MB on x86_64-linux (9 statically-linked Tree-sitter grammars, accepted cost per ADR-0002).
- TypeScript upstream `tags.scm` covers interface / abstract class / method signatures only; plain `class`/`function` are not tagged. Documented in [`wiki/user/commands.md`](wiki/user/commands.md).

[Unreleased]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.7.0
[0.6.1]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.6.1
[0.6.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.6.0
[0.5.3]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.5.3
[0.5.2]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.5.2
[0.5.1]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.5.1
[0.5.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.5.0
[0.4.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.4.0
[0.3.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.3.0
[0.2.1]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.2.1
[0.2.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.2.0
[0.1.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.1.0
