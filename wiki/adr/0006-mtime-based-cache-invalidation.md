# ADR-0006 — mtime-based cache invalidation

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: caching, performance

## Context

Re-indexing a whole repository on every CLI invocation is unacceptable on anything non-trivial. We need a cheap way to decide which files have changed since the last index pass.

## Decision drivers

- Cheap to check (no parsing, no hashing).
- Granular enough to avoid full reparses.
- No external dependency (fsnotify daemons, watchman, git plumbing).
- Survives across CLI invocations.

## Considered options

1. **mtime per file**, stored in SQLite alongside indexed records.
2. **Content hashes** (sha256/blake3) per file.
3. **Git blob OIDs** — only works in a repo with a clean index.
4. **Filesystem watcher** — requires a daemon (out of scope, ADR-0001).

## Decision outcome

Each file's mtime (and size as a tiebreaker) is persisted in SQLite. A re-index pass walks the working tree, compares (mtime, size) per path, and reparses only files whose tuple changed (or which are new). Deleted paths are detected by absence and pruned.

## Positive consequences

- O(files) stat call per invocation; no parsing on the unchanged majority.
- No daemon required; aligns with ADR-0001 (CLI-first).

## Negative consequences

- mtime can lie (clock skew, `touch` without content change, FS quirks). For an AI-context tool, the failure mode is "stale answer" rather than "wrong code shipped" — acceptable.
- mtime granularity varies by filesystem (some report whole seconds). A same-size edit within one granularity window can be missed; the `size` tiebreaker catches most real edits, `--force` catches the rest. We store nanoseconds and use whatever precision the platform gives.
- If users hit mtime-skew issues, a `--force` reindex escape hatch is the answer, not switching to hashes by default.

## Links

- ADR-0003 (SQLite source of truth) — stores mtime per file.
- ADR-0007 (incremental file-local updates) — consumes invalidation signal.
