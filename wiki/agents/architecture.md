# Architecture

Repo shape, data flow, key modules. Keep this **descriptive of the current state**, not aspirational. For the _why_ behind these choices, see [`../adr/`](../adr/).

## Status

v0.2.0 shipped 2026-06-11. M0 + M1 + M1.5 functional surface in place. M2 (LSP daemon) deferred — see [`status.md`](status.md) and the M2 epic in Radicle.

## Current layout

```
crates/
  repoctx/         # CLI binary: clap dispatch, command handlers, output rendering
  index/           # Tree-sitter parsing + symbol extraction (primary backend, ADR-0002)
  store/           # SQLite schema, migrations, queries; source of truth (ADR-0003)
  backend/         # CodeIntelBackend trait + types (ADR-0004)
  integrations/    # repoctx hook: per-agent install (manifest + fetcher + installer)
integrations/      # source content fetched at install time (claude/codex/opencode + shared/)
wiki/              # docs (this tree)
scripts/           # build / dev / bench helpers
.github/workflows/ # ci (3-OS matrix), release (4-target binary build)
```

Future (post-M0, when ADR-0005 lands):

```
crates/
  repoctxd/        # long-lived daemon: workspace registry, LSP child-process lifecycle
  lsp/             # LSP-backed CodeIntelBackend impl (used by repoctxd)
  ipc/             # CLI ↔ repoctxd protocol over unix socket
```

## Commands

Milestone split lives in Radicle issues (M0 epic `e408787`, M1 epic `8ce08ce`, M1.5 epic `b497f7f`, M2 placeholder `58b45d5`).

| Command | Milestone | Backend / module | Notes |
|---------|-----------|------------------|-------|
| `repoctx index`             | M0   | walks tree, parses, writes `store` | full or incremental (ADR-0006, ADR-0007) |
| `repoctx status`            | M0   | reads `store` | counts, freshness, index health |
| `repoctx symbols <query>`   | M0   | `workspace_symbols` | case-insensitive substring across all files |
| `repoctx gain` / `gain top` | M0   | `store::gain` | navigation cost avoided, aggregates only |
| `repoctx outline <file>`    | M1   | `document_symbols` | indented tree (human) / flat (machine) |
| `repoctx definition <name>` | M1   | `workspace_symbols` + exact-name + kind whitelist | name-based; position-based `definition` waits for LSP |
| `repoctx context <symbol>`  | M1   | composite | symbol + source window read from disk + `stale` flag |
| `repoctx hook list / status / install <agent>` | M1.5 | `integrations` crate | per-agent install via GitHub raw + XDG cache |

Every command emits **TOON** by default for non-TTY output, **`--json`** for JSON, **`--toon`** to force TOON on a TTY (ADR-0008).

Future (ADR-0005, via `repoctxd`):

- `repoctx refs <symbol>`
- `repoctx hover <file:line:col>`
- `repoctx callers <file:line:col>`

## Data flow

### Read path (M0 + M1)

1. **Index** — `repoctx index` walks the repo (respecting `.gitignore` via the `ignore` crate), hands files to `index` (Tree-sitter, parallelized with `rayon`), and writes symbols into `store` (SQLite). Per-file `(mtime_ns, size)` is recorded.
2. **Query** — `repoctx symbols|outline|definition|context` opens the `store`, executes the request via the `CodeIntelBackend` (Tree-sitter-backed in M0/M1), and emits human / TOON / JSON.
3. **Incremental update** — on subsequent `repoctx index` runs, `(mtime_ns, size)` comparison against `store.files` decides which files to reparse (ADR-0006). Only changed files are re-indexed; CASCADE on `files.path` drops their old symbols inside the same transaction (ADR-0007). Deleted paths are detected by absence and pruned.
4. **Auto-index** — read commands (`symbols`, `outline`, `definition`, `context`, `status`, `gain`) check for `.repoctx/index.db` at start. Missing → silently run `index` first (one progress line on stderr). `--no-auto-index` opts out.
5. **Gain recording** — each read command tail-records a `usage` row in `store` (aggregates only — no filenames, no query body unless `--record-query`). `repoctx gain` aggregates those rows. ADR-0003 schema v2.

### Hook (M1.5) — install path

1. CLI dispatches to `integrations::Fetcher`. Fetcher checks XDG cache (`<XDG_CACHE_HOME>/repoctx/integrations/<ref>/<agent>/`) — override via `REPOCTX_INTEGRATIONS_CACHE_DIR`.
2. On cache miss, ureq + rustls GETs `https://raw.githubusercontent.com/mikolajmikolajczyk/repoctx/<ref>/integrations/<agent>/<path>`. Default `<ref> = v<CARGO_PKG_VERSION>`; `--ref` overrides.
3. Parsed manifest drives `integrations::Installer`. For each `[[file]]`: fetch source, substitute `{REPOCTX_BIN}` / `{REPO_NAME}` / `{REPO_ROOT}`, dispatch on mode (`write` / `append` / `merge-section`).
4. CLI emits the install summary + per-file removal recipe via the standard render layer.

## Key modules

- **`repoctx` (bin)** — CLI entry. `clap` parsing, dispatch, output formatting (human + TOON + JSON, per ADR-0008). Each command has its own `*_cmd.rs` module under `crates/repoctx/src/`.
- **`index`** — Tree-sitter parser registry + symbol extraction via upstream `tags.scm` / `locals.scm` (plus a small custom query for Markdown/JSON/YAML/TOML where no upstream tags ship). Pure file → records.
- **`store`** — SQLite schema, migrations, query helpers. The only module that touches the DB. Schema v2 (files, symbols, meta, usage). `BEGIN IMMEDIATE` on migration so parallel indexers serialize cleanly.
- **`backend`** — `CodeIntelBackend` trait + query/result types (`SymbolQuery`, `PositionQuery`, `Symbol`, `Location`, `HoverInfo`). One impl: `TreeSitterBackend`, reading from `store`.
- **`integrations`** — `repoctx hook` support. Manifest schema (TOML), HTTP fetcher (ureq + rustls + XDG cache), installer (three modes + template substitution). Public `AGENTS` constant lists supported agents.

Post-M0:

- **`repoctxd`** — workspace registry + LSP lifecycle, listens on a unix socket.
- **`lsp`** — `CodeIntelBackend` impl that proxies to a managed LSP server; lives inside `repoctxd`.
- **`ipc`** — framed-JSON protocol shared by CLI and daemon.

## Distribution

- **Binary**: GitHub Releases. `.github/workflows/release.yml` builds on every `v*` tag for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. Tar.gz / zip + sha256 sidecar. Uploaded via `softprops/action-gh-release@v2`.
- **Source**: `cargo install --git ... --tag v<version>` or `nix run github:.../repoctx`.
- **Integrations content** ships *out* of the binary — fetched at hook-install time from the GitHub mirror so updates don't require a binary rebuild.

## Initial language set (ADR-0002)

Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown. Grammars statically linked into the `repoctx` binary; no plugin system. ~17 MB stripped Linux binary.

## Layering rules

- `repoctx` (bin) → `backend`, `index`, `store`, `integrations`. CLI may touch any layer for dispatch.
- `backend` impls → `store` (read).
- `index` → `store` (write). `index` must not depend on `backend`.
- `store` → no internal deps. Pure persistence.
- `integrations` → no internal deps. Pure manifest + HTTP + fs.
- `lsp` (future) → `backend` (implements the trait). May not depend on `index`.
- `repoctxd` (future) → `backend`, `lsp`, `ipc`. Never imported by `repoctx` the CLI.

Enforce informally for now; revisit `cargo deny` or a layering lint once the workspace stabilizes. ADR-0001 (CLI-first) and ADR-0004 (backend abstraction) are the load-bearing decisions here.
