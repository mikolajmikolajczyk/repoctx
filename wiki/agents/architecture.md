# Architecture

Repo shape, data flow, key modules. Keep this **descriptive of the current state**, not aspirational. For the _why_ behind these choices, see [`../adr/`](../adr/).

## Status

Pre-alpha scaffold. Concrete crate layout will be committed during `milestone:m0-foundation`. Treat the sketch below as the **target shape** for M0 — update this file as code lands.

## Target layout (M0)

```
crates/
  repoctx/        # CLI binary: clap dispatch, command handlers, human/JSON output
  index/          # Tree-sitter parsing + symbol extraction (primary backend, ADR-0002)
  store/          # SQLite schema, migrations, queries; source of truth (ADR-0003)
  backend/        # CodeIntelBackend trait + types + registry (ADR-0004)
wiki/             # docs (this tree)
scripts/          # build / dev helpers
```

Future (post-M0, when ADR-0005 lands):

```
crates/
  repoctxd/       # long-lived daemon: workspace registry, LSP child-process lifecycle
  lsp/            # LSP-backed CodeIntelBackend impl (used by repoctxd)
  ipc/            # CLI ↔ repoctxd protocol over unix socket
```

## Commands (M0)

| Command | Backend method | Notes |
|---------|----------------|-------|
| `repoctx index`            | walks tree, parses, writes `store` | full or incremental (ADR-0006, ADR-0007) |
| `repoctx status`           | reads `store` | counts, freshness, index health |
| `repoctx symbols <query>`  | `workspace_symbols` | substring/fuzzy match across all files |
| `repoctx definition <q>`   | `definition` | best-effort from Tree-sitter; richer with LSP |
| `repoctx outline <file>`   | `document_symbols` | structured file contents |
| `repoctx context <symbol>` | composite | symbol + surrounding code window for agents |

Every command emits **TOON** by default for non-TTY output, **`--json`** for JSON, **`--toon`** to force TOON on a TTY (ADR-0008).

Future (ADR-0005, via `repoctxd`):

- `repoctx refs <symbol>`
- `repoctx hover <file:line:col>`
- `repoctx callers <file:line:col>`

## Data flow

1. **Index** — `repoctx index` walks the repo (respecting `.gitignore` via the `ignore` crate), hands files to `index` (Tree-sitter, parallelized with `rayon`), and writes symbols into `store` (SQLite). Per-file mtime is recorded.
2. **Query** — `repoctx symbols|definition|outline|context` opens the `store`, executes the request via the appropriate `CodeIntelBackend` (Tree-sitter-backed in M0), and emits human or JSON output.
3. **Incremental update** — on subsequent `repoctx index` runs, mtime comparison against `store.files` decides which files to reparse (ADR-0006). Only changed files are re-indexed; CASCADE on `files.path` drops their old symbols inside the same transaction (ADR-0007). Deleted paths are detected by absence and pruned.

## Key modules

- **`repoctx` (bin)** — CLI entry. `clap` parsing, dispatch, output formatting (human + TOON + JSON, per ADR-0008).
- **`index`** — Tree-sitter parser registry + symbol extraction via upstream `tags.scm` / `locals.scm`. Pure file → records.
- **`store`** — SQLite schema, migrations, query helpers. The only module that touches the DB.
- **`backend`** — `CodeIntelBackend` trait + query/result types (`SymbolQuery`, `PositionQuery`, `Symbol`, `Location`, `HoverInfo`). One impl in M0: `TreeSitterBackend`, reading from `store`.

Post-M0:

- **`repoctxd`** — workspace registry + LSP lifecycle, listens on a unix socket.
- **`lsp`** — `CodeIntelBackend` impl that proxies to a managed LSP server; lives inside `repoctxd`.
- **`ipc`** — framed-JSON protocol shared by CLI and daemon.

## Initial language set (ADR-0002)

Go, Rust, TypeScript, JavaScript, Python, JSON, YAML, TOML, Markdown. Grammars statically linked into the `repoctx` binary; no plugin system.

## Layering rules

- `repoctx` → `backend`, `index`, `store` (CLI may touch any layer for dispatch).
- `backend` impls → `store` (read).
- `index` → `store` (write). `index` must not depend on `backend`.
- `store` → no internal deps. Pure persistence.
- `lsp` (future) → `backend` (implements the trait). May not depend on `index`.
- `repoctxd` (future) → `backend`, `lsp`, `ipc`. Never imported by `repoctx` the CLI.

Enforce informally for now; revisit `cargo deny` or a layering lint once the workspace stabilizes. ADR-0001 (CLI-first) and ADR-0004 (backend abstraction) are the load-bearing decisions here.
