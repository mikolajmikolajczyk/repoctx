//! Schema versioning + migration application.
//!
//! Schema v1 = ADR-0003 (amended pre-release). New schema versions append
//! migrations; never mutate prior ones.

use rusqlite::{Connection, TransactionBehavior};

use crate::error::{Result, StoreError};

/// Highest schema version this binary supports.
pub const SUPPORTED_VERSION: u32 = 6;

/// Migration scripts indexed by target version. Position N is the SQL to
/// move the DB from version N-1 to version N.
const MIGRATIONS: &[&str] = &[
    // -> v1
    r#"
    CREATE TABLE files (
        path     TEXT PRIMARY KEY,
        mtime_ns INTEGER NOT NULL,
        size     INTEGER NOT NULL,
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

    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT
    );
    "#,
    // -> v2 (gain analytics; epic 4dd57c8)
    //
    // Aggregates only — no filenames, no symbol names, no content. `query`
    // is NULL unless the caller passed `--record-query`.
    r#"
    CREATE TABLE usage (
        id                        INTEGER PRIMARY KEY,
        ts_unix_ns                INTEGER NOT NULL,
        command                   TEXT    NOT NULL,
        candidate_files           INTEGER NOT NULL,
        candidate_bytes           INTEGER NOT NULL,
        estimated_baseline_tokens INTEGER NOT NULL,
        returned_tokens           INTEGER NOT NULL,
        output_format             TEXT    NOT NULL,
        query                     TEXT
    );

    CREATE INDEX usage_ts_idx      ON usage(ts_unix_ns);
    CREATE INDEX usage_command_idx ON usage(command);
    "#,
    // -> v3 (per-repo config; epic 2c96964)
    //
    // Key-value store for persistent CLI behavior. Values are TEXT and
    // parsed at read time by the loader; unknown keys are warned but
    // accepted so older binaries don't brick on settings written by
    // newer ones.
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    "#,
    // -> v4 (static call graph; epic af42572, ADR-0010)
    //
    // One row per call SITE. Edges are stored name-based and caller-located,
    // NOT by symbol id: symbol ids are reassigned on every reindex, so a
    // stored cross-file callee id would dangle when the *other* file is
    // reparsed. Callee resolution is done at query time by joining
    // `callee_name` to `symbols(name)` — unresolved (external) callees simply
    // find no match, ambiguous ones find several. `resolution` is 'syntactic'
    // for these Tree-sitter edges; a future LSP backend writes 'semantic'
    // rows into the same table (ADR-0005). Cascades with the file via
    // file_path FK, so re-indexing a file replaces its edges atomically.
    r#"
    CREATE TABLE calls (
        id                INTEGER PRIMARY KEY,
        file_path         TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
        caller_name       TEXT NOT NULL,
        caller_start_line INTEGER NOT NULL,
        callee_name       TEXT NOT NULL,
        site_line         INTEGER NOT NULL,
        site_column       INTEGER NOT NULL,
        resolution        TEXT NOT NULL DEFAULT 'syntactic'
    );

    CREATE INDEX calls_callee_idx    ON calls(callee_name);
    CREATE INDEX calls_caller_idx    ON calls(caller_name);
    CREATE INDEX calls_file_path_idx ON calls(file_path);
    "#,
    // -> v5 (import / dependency graph; epic #4, ADR-0011)
    //
    // One row per import SITE: the importing file plus the raw module
    // specifier as written in source (`@adapters/storage-idb`, `./foo`,
    // `std::collections::HashMap`, `os`, `<stdio.h>` already de-bracketed).
    // Like `calls`, edges are name/string-based and resolved at query time —
    // precise specifier→file resolution (tsconfig paths, node_modules, crate
    // layout) is deferred to a future resolver writing 'semantic' rows here
    // (ADR-0011 mirrors ADR-0010). Cascades with the file via file_path FK.
    r#"
    CREATE TABLE imports (
        id          INTEGER PRIMARY KEY,
        file_path   TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
        module      TEXT NOT NULL,
        site_line   INTEGER NOT NULL,
        site_column INTEGER NOT NULL,
        resolution  TEXT NOT NULL DEFAULT 'syntactic'
    );

    CREATE INDEX imports_module_idx    ON imports(module);
    CREATE INDEX imports_file_path_idx ON imports(file_path);
    "#,
    // -> v6 (hook passthrough telemetry; issue #7)
    //
    // One row per grep/rg/find command the PreToolUse hook saw, bucketed by
    // `idiom` (bare-ident / regex / call-shape / import-shape / …) and
    // `outcome` (rewritten / passthrough / chained). Aggregate-only: NO
    // command body, NO pattern, NO paths — same privacy posture as `usage`.
    // Powers `repoctx discover`, which ranks idioms by adoption gap so hook
    // rewrites can be widened from real data, not guesses.
    r#"
    CREATE TABLE hook_events (
        id         INTEGER PRIMARY KEY,
        ts_unix_ns INTEGER NOT NULL,
        tool       TEXT NOT NULL,
        idiom      TEXT NOT NULL,
        outcome    TEXT NOT NULL
    );

    CREATE INDEX hook_events_idiom_idx ON hook_events(idiom);
    "#,
];

pub fn read_version(conn: &Connection) -> Result<u32> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Ok(0);
    }
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    // Fast path: peek before locking. Avoids contention when the DB is
    // already up to date (the overwhelming case).
    let peek = read_version(conn)?;
    if peek == SUPPORTED_VERSION {
        return Ok(());
    }
    if peek > SUPPORTED_VERSION {
        return Err(StoreError::NewerSchema {
            db_version: peek,
            supported: SUPPORTED_VERSION,
        });
    }

    // Acquire a write lock NOW; if another process is mid-migration the
    // busy_timeout (5s) covers serialization. Then re-read the version
    // inside the transaction so we don't reapply a migration the winner
    // already committed.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_version_tx(&tx)?;
    if current > SUPPORTED_VERSION {
        return Err(StoreError::NewerSchema {
            db_version: current,
            supported: SUPPORTED_VERSION,
        });
    }
    for target in (current + 1)..=SUPPORTED_VERSION {
        let sql = MIGRATIONS[(target - 1) as usize];
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [target.to_string()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn read_version_tx(tx: &rusqlite::Transaction<'_>) -> Result<u32> {
    let exists: bool = tx
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Ok(0);
    }
    let v: Option<String> = tx
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
}
