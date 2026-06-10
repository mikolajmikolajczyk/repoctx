# ADR-0007 — Incremental, file-local repository updates

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: indexing, performance

## Context

Given mtime-based invalidation (ADR-0006), we need a strategy for applying changes to the SQLite store. The naive approach — wipe-and-rebuild for any change — defeats the whole point.

## Decision drivers

- A single-file edit should re-index only that file.
- Updates must be transactional: partial failure must not corrupt the store.
- The strategy must scale to many small edits (typical editing session).

## Considered options

1. **File-local upserts in a single transaction.** Delete prior records keyed by `file_id`, insert new ones.
2. **Whole-repo rebuild** on any change.
3. **Diff-level granularity** (per-symbol upserts).

## Decision outcome

The update unit is one file. For each changed file (as detected by ADR-0006), within one transaction: delete all records keyed by that file's `file_id`, reparse, insert fresh records, update the file's mtime/size row. Deleted files have their rows cascaded away by FK or explicitly purged. New files get inserted records and a fresh mtime row.

## Positive consequences

- Re-index cost scales with changed files, not repo size.
- Transactional per-file updates keep the DB consistent even on crash.
- Simple mental model: a file's index entry equals the latest parse of that file, or nothing.

## Negative consequences

- Cross-file relationships (e.g. references from A→B when only A changes) need to be recomputed on the changed side and may temporarily lag on the unchanged side. Acceptable; queries can join against the freshest data each side has.

## Links

- ADR-0003 (SQLite source of truth), ADR-0006 (mtime invalidation).
