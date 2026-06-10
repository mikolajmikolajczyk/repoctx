# ADR-0003 — SQLite as source of truth for repository metadata

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: storage, persistence, schema

## Context

`repoctx` indexes files and symbols across what may be a large repository. The CLI is stateless between invocations (ADR-0001), so durable state must live on disk. Queries must be fast and scriptable; indexing must support incremental updates (ADR-0007) keyed on mtime (ADR-0006).

## Decision drivers

- Zero-config, embedded, mature Rust support (`rusqlite`).
- Transactional updates so incremental re-index can't leave the DB inconsistent.
- Real query language (SQL) — backends and commands compose queries directly.
- File-based: easy to inspect (`sqlite3` CLI), copy, throw away.

## Considered options

1. **SQLite** as the authoritative store.
2. **Flat-file caches** (JSON / Bincode) per file or per repo.
3. **Embedded key-value store** (sled, redb).
4. **External DB** (Postgres) — overkill for a local CLI.

## Decision outcome

**SQLite is the source of truth.** All durable metadata — files, symbols, mtimes, schema version — lives in a single SQLite database under `.repoctx/` at the repo root.

### MVP schema sketch

```sql
CREATE TABLE files (
    path     TEXT PRIMARY KEY,
    mtime    INTEGER NOT NULL,
    language TEXT NOT NULL
);

CREATE TABLE symbols (
    id           INTEGER PRIMARY KEY,
    file_path    TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL,
    start_line   INTEGER NOT NULL,
    start_column INTEGER NOT NULL,
    end_line     INTEGER NOT NULL,
    end_column   INTEGER NOT NULL
);

CREATE INDEX symbols_name_idx      ON symbols(name);
CREATE INDEX symbols_file_path_idx ON symbols(file_path);
```

Schema is owned by the `store` crate; migrations are versioned and applied on open. Anything richer (cross-file refs, semantic facts) lands in additional tables, not by mutating these.

## Positive consequences

- One file to back up, inspect, or delete to reset.
- Real indexes + transactions; complex queries stay fast.
- `ON DELETE CASCADE` keeps file-local updates (ADR-0007) trivial: delete the `files` row, symbols vanish.
- Mature Rust ecosystem; no bespoke storage layer.

## Negative consequences

- Schema migrations require care; we own the migration path.
- Single-writer model — parallel writers need application-level coordination (not a concern for MVP).

## Links

- ADR-0001 (CLI-first) — SQLite is what makes stateless invocations viable.
- ADR-0006 (mtime invalidation) — `files.mtime` column is the invalidation signal.
- ADR-0007 (incremental file-local updates) — implemented as transactional upserts keyed on `files.path`.
