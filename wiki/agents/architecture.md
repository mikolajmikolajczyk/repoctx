# Architecture

Repo shape, data flow, key modules. Keep this **descriptive of the current state**, not aspirational. For the _why_ behind these choices, see [`../adr/`](../adr/).

## Status

v0.11.4 (2026-06-17): static **call graph** (`callers`/`callees`/`callgraph`, core-8 langs, ADR-0010, schema v4 `calls`) + **call-graph analyses** (`deadcode`/`impact`/`cycles`, issue #3) + **import / dependency graph** (`deps`/`rdeps`/`boundary`, ADR-0011, schema v5 `imports`) + **`repoctx search`** (textually-complete) + **`repoctx prime`** (session-start orientation digest). CLI surface complete: indexing, search, navigation, call graph, import graph, `repoctx init` onboarding (guidance files + a Claude **SessionStart** hook that runs `repoctx prime`, `-g` global, `--uninstall`), embedded per-agent install, per-repo config, language-coverage advisory. Adoption is via **priming** — the agent is oriented once at session start, not by intercepting every command (decisions `2026-06-16-adoption-via-priming` + `2026-06-17-remove-pretooluse-hook`). LSP daemon deferred — see [`status.md`](status.md) and the daemon epic in GitHub issues.

## Current layout

```
crates/
  repoctx/         # CLI binary: clap dispatch, command handlers, output rendering
  index/           # Tree-sitter parsing + symbol extraction (primary backend, ADR-0002)
  store/           # SQLite schema, migrations, queries; source of truth (ADR-0003)
  backend/         # CodeIntelBackend trait + types (ADR-0004)
  integrations/    # per-agent guidance install (embedded manifest + installer)
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
| `repoctx import-cycles` / `modules` | `modulegraph_cmd` + petgraph | resolved file→file import graph: circular imports (SCC), topology + build order (toposort); relative-resolved, alias edges external |
| `repoctx overview` | `overview_cmd` + `store` counts/aggregates | repo architecture in one call (issue #5): totals, languages, module sizes, entry points, hotspots; composes index + call graph |
| `repoctx changed [--since REF]` | `changed_cmd` + git diff + `backend::callers` | change-aware blast radius (issue #6): changed symbols + transitive callers |
| `repoctx gain` / `gain top` | `store::gain` | navigation cost avoided, aggregates only |
| `repoctx outline <file>`    | `document_symbols` | indented tree (human) / flat (machine) |
| `repoctx definition <name>` | `workspace_symbols` + exact-name + kind whitelist | name-based; position-based `definition` waits for LSP |
| `repoctx context <symbol>`  | composite | symbol + source window read from disk + `stale` flag |
| `repoctx prime`             | `prime_cmd` + `store` counts/aggregates | compact session-start orientation digest (markdown); what the SessionStart hook injects |
| `repoctx init [-g]` | `init_cmd` + `session_hook` + `integrations` crate | install guidance files + (for Claude) a SessionStart hook running `repoctx prime`; `--uninstall` |

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
5. **Config layer** — every persistent CLI behavior (`gain.no_record`, `gain.record_query`, `output.default`, `index.nested_keys`, `analysis.subsystem_min_size`) lives in a `settings` key/value table inside the same `.repoctx/index.db`. `Config::load(&Store)` resolves four sources in order — CLI flag, env var (`REPOCTX_<SECTION>_<KEY>`), settings row, built-in default — and tracks each value's `Source` for diagnostics. Schema v3 adds the `settings` table; older DBs migrate transparently. (Legacy `hook.*` rows from the removed PreToolUse hook are ignored silently.) See `wiki/user/config.md`.
6. **Onboarding + priming** (`init_cmd` + `session_hook` + `integrations`) — `repoctx init` installs the agent guidance files and, for Claude, adds a **SessionStart** hook entry to `.claude/settings.json` (or `~/.claude/settings.json` with `-g`) that runs `repoctx prime`. At session start Claude Code runs that hook and injects `prime`'s ~600-token orientation digest into the agent's context, so the agent begins primed to use `repoctx` instead of blind `grep`/`cat`. Adoption is via this one-time priming, **not** by intercepting commands — the per-command PreToolUse rewrite hook was removed (decisions `2026-06-16-adoption-via-priming` + `2026-06-17-remove-pretooluse-hook`). A user's own `rtk` (or other) PreToolUse hook is independent of repoctx. `repoctx init --uninstall` removes the SessionStart entry + guidance.
7. **Gain recording** — each read command tail-records a `usage` row in `store` via `gain::emit_and_record` (aggregates only — no filenames, no query body unless `--record-query`). `repoctx gain` aggregates those rows; token figures are bytes/4 estimates. ADR-0003.

### Guidance install path

1. `integrations::content` resolves the agent's embedded manifest + its referenced files (`include_str!` at build time; `../shared/...` normalized, kept inside `integrations/`). No network, no cache.
2. Parsed manifest drives `integrations::Installer`. For each `[[file]]`: read embedded source, substitute `{REPOCTX_BIN}` / `{REPO_NAME}` / `{REPO_ROOT}`, dispatch on mode (`write` / `append` / `merge-section`).
3. CLI emits the install summary + per-file removal recipe via the standard render layer.

## Key modules

- **`repoctx` (bin)** — CLI entry. `clap` parsing, dispatch, output formatting (human + TOON + JSON, per ADR-0008). Each command has its own `*_cmd.rs` module under `crates/repoctx/src/`.
- **`index`** — Tree-sitter parser registry + symbol extraction via upstream `tags.scm` / `locals.scm` (plus a small custom query for Markdown/JSON/YAML/TOML where no upstream tags ship). Also `parse_calls_with`: per-language call-site queries (core-8) → `CallRecord`s, caller resolved by walking up the syntax tree to the enclosing function/method; and `parse_imports`: per-language import queries (core-8) → `ImportRecord`s (file → raw module specifier). Pure file → records.
- **`store`** — SQLite schema, migrations, query helpers. The only module that touches the DB. Schema v9 (files, symbols [+ `visibility`], meta, usage, settings, **calls** [+ `is_method`], **imports**, and the legacy `hook_events` / `hook_samples` tables). Symbols carry a lexical `visibility` (`public`/`private`/`unknown`, per-language; Go so far) that `deadcode` uses to skip exported API (issue #10). The `calls` table holds name-based call edges (caller name + start line, callee name, site, `resolution`, `is_method`); `callers_of`/`callees_of` resolve callees to symbols by name at query time, **receiver-aware** — a method call (`is_method`) binds only to a `method`, never a free `function` (issue #9, ADR-0010). The `imports` table holds string-based import edges (file → raw module specifier, site, `resolution`); `deps_of`/`importers_of` query by file / substring (ADR-0011). The `hook_events` / `hook_samples` tables are **legacy/unused** — they backed the removed `discover` telemetry but stay physically present because migrations are append-only. `BEGIN IMMEDIATE` on migration so parallel indexers serialize cleanly.
- **onboarding subsystem (in `repoctx` bin)** — `init_cmd` (`init` / `--uninstall`), `session_hook` (SessionStart settings.json wiring that runs `repoctx prime`), `prime_cmd` (the orientation digest). Adoption is via priming, not command interception.
- **`backend`** — `CodeIntelBackend` trait + query/result types (`SymbolQuery`, `PositionQuery`, `Symbol`, `Location`, `HoverInfo`, `CallEdge`). `callers`/`callees` are name-based (served by Tree-sitter today). One impl: `TreeSitterBackend`, reading from `store`.
- **`integrations`** — per-agent guidance install support. Manifest schema (TOML), embedded content (`content` module, `include_str!`), installer (three modes + template substitution). Public `AGENTS` constant lists supported agents. No network/HTTP deps.

Future:

- **`repoctxd`** — workspace registry + LSP lifecycle, listens on a unix socket.
- **`lsp`** — `CodeIntelBackend` impl that proxies to a managed LSP server; lives inside `repoctxd`.
- **`ipc`** — framed-JSON protocol shared by CLI and daemon.

## Distribution

- **Binary**: GitHub Releases. `.github/workflows/release.yml` builds on every `v*` tag for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.
- **Source**: `cargo install --git ... --tag v<version>` or `nix run github:.../repoctx`.
- **Integrations content** is embedded *into* the binary (`include_str!`). `repoctx init` is offline + version-locked; updating the guidance content means shipping a new binary.

## Language set

Initial (ADR-0002): Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown. v0.7.0 batch (language-coverage epic): Ruby, C, C++, Java, C#, PHP, Lua, Kotlin, Swift, Bash. 20 languages total. Grammars statically linked into the `repoctx` binary; no plugin system (see `wiki/decisions/2026-06-13-grammar-loading-strategy.md`). ~32 MB stripped Linux binary. JSON/YAML/TOML are top-level-key by default with opt-in all-depth extraction (`index.nested_keys`).

## Layering rules

- `repoctx` (bin) → `backend`, `index`, `store`, `integrations`. CLI may touch any layer for dispatch.
- `backend` impls → `store` (read).
- `index` → `store` (write). `index` must not depend on `backend`.
- `store` → no internal deps. Pure persistence.
- `integrations` → no internal deps. Pure manifest + fs (no network).
- `lsp` (future) → `backend` (implements the trait). May not depend on `index`.
- `repoctxd` (future) → `backend`, `lsp`, `ipc`. Never imported by `repoctx` the CLI.

Enforce informally for now; revisit `cargo deny` or a layering lint once the workspace stabilizes. ADR-0001 (CLI-first) and ADR-0004 (backend abstraction) are the load-bearing decisions here.
