# Architecture

Repo shape, data flow, key modules. Keep this **descriptive of the current state**, not aspirational. For the _why_ behind these choices, see [`../adr/`](../adr/).

## Status

v0.11.4 (2026-06-16): static **call graph** (`callers`/`callees`/`callgraph`, core-8 langs, ADR-0010, schema v4 `calls`) + **call-graph analyses** (`deadcode`/`impact`/`cycles`, issue #3) + **import / dependency graph** (`deps`/`rdeps`/`boundary`, ADR-0011, schema v5 `imports`) + **`repoctx search`** (textually-complete; the hook rewrites `rg <ident>` here) + **hook passthrough telemetry** (`discover`, schema v6 `hook_events` / v7 `hook_samples`, issue #7). CLI surface complete: indexing, search, navigation, call graph, import graph, `repoctx init` meta-hook (committed script + in-binary rewrite/rtk-chain + doctor + uninstall, `-g` global skill, guidance-only when a global hook exists), embedded per-agent install, per-repo config, language-coverage advisory. LSP daemon deferred — see [`status.md`](status.md) and the daemon epic in GitHub issues.

## Current layout

```
crates/
  repoctx/         # CLI binary: clap dispatch, command handlers, output rendering
  index/           # Tree-sitter parsing + symbol extraction (primary backend, ADR-0002)
  store/           # SQLite schema, migrations, queries; source of truth (ADR-0003)
  backend/         # CodeIntelBackend trait + types (ADR-0004)
  integrations/    # repoctx hook: per-agent install (embedded manifest + installer)
integrations/      # source content embedded into the binary at build time (claude/codex/opencode + shared/)
wiki/              # docs (this tree)
scripts/           # build / dev / bench helpers
.github/workflows/ # ci (3-OS matrix), release (4-target binary build)
```

Future, when ADR-0005 lands:

```
crates/
  repoctxd/        # long-lived daemon: workspace registry, LSP child-process lifecycle
  lsp/             # LSP-backed CodeIntelBackend impl (used by repoctxd)
  ipc/             # CLI ↔ repoctxd protocol over unix socket
```

## Commands

Tracked across the foundation epic, navigation epic, integrations epic, and daemon placeholder epic (GitHub issues).

| Command | Backend / module | Notes |
|---------|------------------|-------|
| `repoctx index`             | walks tree, parses, writes `store` | full or incremental (ADR-0006, ADR-0007) |
| `repoctx status`            | reads `store` | counts, freshness, index health |
| `repoctx symbols <query>`   | `workspace_symbols` | case-insensitive substring across all files |
| `repoctx search <pattern>`  | `search_cmd` | symbol defs + real-ripgrep textual matches, compressed; `--lang`, `--limit` |
| `repoctx callers <name>` / `callees <name>` | `callgraph_cmd` + `backend::callers/callees` | direct static call-graph edges (name-based, ADR-0010) |
| `repoctx callgraph <name>`  | `callgraph_cmd` | transitive traversal; `--depth`, `--direction up\|down\|both`; cycle-safe |
| `repoctx deadcode` / `impact <name>` / `cycles` | `analysis_cmd` + `store::uncalled_symbols/resolved_edge_pairs` | Tier-1 call-graph analyses (issue #3): uncalled defs, blast radius, cycle detection |
| `repoctx deps <file>` / `rdeps <module>` / `boundary --from --to` | `deps_cmd` + `store::deps_of/importers_of/boundary_crossings` | import / dependency graph (string-based, ADR-0011); `boundary` lists layer crossings, `--forbid` = CI gate |
| `repoctx gain` / `gain top` | `store::gain` | navigation cost avoided, aggregates only |
| `repoctx discover`          | `discover_cmd` + `store::hook_event_stats` | hook passthrough telemetry — adoption gap per grep idiom (issue #7) |
| `repoctx outline <file>`    | `document_symbols` | indented tree (human) / flat (machine) |
| `repoctx definition <name>` | `workspace_symbols` + exact-name + kind whitelist | name-based; position-based `definition` waits for LSP |
| `repoctx context <symbol>`  | composite | symbol + source window read from disk + `stale` flag |
| `repoctx init [-g]` | `init_cmd` + `hook_script` + `hook_scan` | install the meta-hook (committed script + settings entry + guidance); race detection; `--uninstall` |
| `repoctx hook claude [--rtk-chain]` | `hook_rewrite` | PreToolUse handler: semantic rewrite, else chain rtk, else passthrough |
| `repoctx hook doctor [-g] [--fix]` | `init_cmd` + `hook_script` | drift/tamper check + repair |
| `repoctx hook list / status / install <agent>` | `integrations` crate | per-agent guidance install from binary-embedded content (no network) |
| `repoctx rewrite <cmd>` | `hook_rewrite` | show the rewrite decision (debug/bench) |

Every command emits **TOON** by default for non-TTY output, **`--json`** for JSON, **`--toon`** to force TOON on a TTY (ADR-0008).

Future (ADR-0005, via `repoctxd`):

- `repoctx refs <symbol>`
- `repoctx hover <file:line:col>`
- *semantic* call edges (`resolution = 'semantic'`) written into the same
  `calls` table the static graph already uses — no schema fork (ADR-0010).

## Data flow

### Read path

1. **Index** — `repoctx index` walks the repo (respecting `.gitignore` via the `ignore` crate), hands files to `index` (Tree-sitter, parallelized with `rayon`), and writes symbols into `store` (SQLite). Per-file `(mtime_ns, size)` is recorded.
2. **Query** — `repoctx symbols|outline|definition|context` opens the `store`, executes the request via the `CodeIntelBackend` (Tree-sitter-backed today), and emits human / TOON / JSON.
3. **Incremental update** — on subsequent `repoctx index` runs, `(mtime_ns, size)` comparison against `store.files` decides which files to reparse (ADR-0006). Only changed files are re-indexed; CASCADE on `files.path` drops their old symbols inside the same transaction (ADR-0007). Deleted paths are detected by absence and pruned.
4. **Auto-index / auto-reindex** — `symbols` / `outline` / `definition` / `context` run an incremental `index` pass before answering (via `read_cmd::ensure_fresh`): missing DB triggers a from-scratch build, present DB triggers an mtime+size delta pass that only reparses changed files. `status` and `gain` use the lighter `ensure_db` — they only build the DB if missing; they never auto-reindex on top of one, because `status`'s job is to report staleness and `gain` only reads the `usage` table. There's no opt-out — indexing is core to the tool.
5. **Config layer** — every persistent CLI behavior (`hook.rewrite`, `hook.use_rtk`, `hook.chainable`, `gain.no_record`, `gain.record_query`, `output.default`, `index.nested_keys`) lives in a `settings` key/value table inside the same `.repoctx/index.db`. `Config::load(&Store)` resolves four sources in order — CLI flag, env var (`REPOCTX_<SECTION>_<KEY>`), settings row, built-in default — and tracks each value's `Source` for diagnostics. Schema v3 adds the `settings` table; older DBs migrate transparently. (`hook.chain_commands` is a legacy v0.5.x key, migrated away by `repoctx init`.) See `wiki/user/config.md`.
6. **Meta-hook** (`init_cmd` + `hook_script` + `hook_scan` + `hook_rewrite`) — `repoctx init` writes a committed dumb-pipe script (`.repoctx/hook.sh`, or `~/.claude/repoctx-hook.sh` with `-g`) and points `.claude/settings.json`'s sole `PreToolUse → Bash` entry at it. The script (no `jq`) just `exec`s `repoctx hook claude --rtk-chain=$RTK_CHAIN`. The handler tries a conservative semantic rewrite (`rg <ident>`, `rg "fn <ident>"`, `grep -r/-rn` variants); on miss, if chaining is on, it runs the first allowlisted tool on PATH (`hook.chainable`, default rtk) and forwards its output; else exits 1 (silent passthrough). `init` refuses configurations that would race (foreign hook anywhere, or a repoctx/rtk hook in a scope that double-fires) unless `--force`; the one exception is a project `init` under an existing global repoctx hook, which installs guidance only (skill + CLAUDE.md) and skips the redundant project hook. `repoctx hook doctor` re-renders the expected script and reports/repairs drift. Decision doc: `wiki/decisions/2026-06-13-repoctx-init.md`.
7. **Gain recording** — each read command tail-records a `usage` row in `store` via `gain::emit_and_record` (aggregates only — no filenames, no query body unless `--record-query`). `repoctx gain` aggregates those rows; token figures are bytes/4 estimates. ADR-0003.

### Hook install path

1. `integrations::content` resolves the agent's embedded manifest + its referenced files (`include_str!` at build time; `../shared/...` normalized, kept inside `integrations/`). No network, no cache.
2. Parsed manifest drives `integrations::Installer`. For each `[[file]]`: read embedded source, substitute `{REPOCTX_BIN}` / `{REPO_NAME}` / `{REPO_ROOT}`, dispatch on mode (`write` / `append` / `merge-section`).
3. CLI emits the install summary + per-file removal recipe via the standard render layer.

## Key modules

- **`repoctx` (bin)** — CLI entry. `clap` parsing, dispatch, output formatting (human + TOON + JSON, per ADR-0008). Each command has its own `*_cmd.rs` module under `crates/repoctx/src/`.
- **`index`** — Tree-sitter parser registry + symbol extraction via upstream `tags.scm` / `locals.scm` (plus a small custom query for Markdown/JSON/YAML/TOML where no upstream tags ship). Also `parse_calls_with`: per-language call-site queries (core-8) → `CallRecord`s, caller resolved by walking up the syntax tree to the enclosing function/method; and `parse_imports`: per-language import queries (core-8) → `ImportRecord`s (file → raw module specifier). Pure file → records.
- **`store`** — SQLite schema, migrations, query helpers. The only module that touches the DB. Schema v6 (files, symbols, meta, usage, settings, **calls**, **imports**, **hook_events**). The `calls` table holds name-based call edges (caller name + start line, callee name, site, `resolution`); `callers_of`/`callees_of` resolve callees to symbols by name at query time (ADR-0010). The `imports` table holds string-based import edges (file → raw module specifier, site, `resolution`); `deps_of`/`importers_of` query by file / substring (ADR-0011). The `hook_events` table holds aggregate hook telemetry (tool, idiom, outcome — no command bodies); `record_hook_event`/`hook_event_stats` power `repoctx discover` (issue #7). `BEGIN IMMEDIATE` on migration so parallel indexers serialize cleanly.
- **hook subsystem (in `repoctx` bin)** — `init_cmd` (`init` / `doctor` / `--uninstall`), `hook_script` (embedded `hook.sh` template + render), `hook_scan` (cross-scope classify + race ruleset), `hook_marker` (fingerprint reader), `hook_rewrite` (PreToolUse handler + rtk chain), `hook_takeover` (settings.json writers).
- **`backend`** — `CodeIntelBackend` trait + query/result types (`SymbolQuery`, `PositionQuery`, `Symbol`, `Location`, `HoverInfo`, `CallEdge`). `callers`/`callees` are name-based (served by Tree-sitter today). One impl: `TreeSitterBackend`, reading from `store`.
- **`integrations`** — `repoctx hook` support. Manifest schema (TOML), embedded content (`content` module, `include_str!`), installer (three modes + template substitution). Public `AGENTS` constant lists supported agents. No network/HTTP deps.

Future:

- **`repoctxd`** — workspace registry + LSP lifecycle, listens on a unix socket.
- **`lsp`** — `CodeIntelBackend` impl that proxies to a managed LSP server; lives inside `repoctxd`.
- **`ipc`** — framed-JSON protocol shared by CLI and daemon.

## Distribution

- **Binary**: GitHub Releases. `.github/workflows/release.yml` builds on every `v*` tag for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.
- **Source**: `cargo install --git ... --tag v<version>` or `nix run github:.../repoctx`.
- **Integrations content** is embedded *into* the binary (`include_str!`). `hook install` is offline + version-locked; updating the guidance content means shipping a new binary.

## Language set

Initial (ADR-0002): Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown. v0.7.0 batch (language-coverage epic): Ruby, C, C++, Java, C#, PHP, Lua, Kotlin, Swift, Bash. 20 languages total. Grammars statically linked into the `repoctx` binary; no plugin system (see `wiki/decisions/2026-06-13-grammar-loading-strategy.md`). ~32 MB stripped Linux binary. JSON/YAML/TOML are top-level-key by default with opt-in all-depth extraction (`index.nested_keys`).

## Layering rules

- `repoctx` (bin) → `backend`, `index`, `store`, `integrations`. CLI may touch any layer for dispatch.
- `backend` impls → `store` (read).
- `index` → `store` (write). `index` must not depend on `backend`.
- `store` → no internal deps. Pure persistence.
- `integrations` → no internal deps. Pure manifest + HTTP + fs.
- `lsp` (future) → `backend` (implements the trait). May not depend on `index`.
- `repoctxd` (future) → `backend`, `lsp`, `ipc`. Never imported by `repoctx` the CLI.

Enforce informally for now; revisit `cargo deny` or a layering lint once the workspace stabilizes. ADR-0001 (CLI-first) and ADR-0004 (backend abstraction) are the load-bearing decisions here.
