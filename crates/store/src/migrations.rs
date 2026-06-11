//! Schema versioning + migration application.
//!
//! Schema v1 = ADR-0003 (amended pre-release). New schema versions append
//! migrations; never mutate prior ones.

use rusqlite::Connection;

use crate::error::{Result, StoreError};

/// Highest schema version this binary supports.
pub const SUPPORTED_VERSION: u32 = 1;

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
    let current = read_version(conn)?;
    if current > SUPPORTED_VERSION {
        return Err(StoreError::NewerSchema {
            db_version: current,
            supported: SUPPORTED_VERSION,
        });
    }
    for target in (current + 1)..=SUPPORTED_VERSION {
        let sql = MIGRATIONS[(target - 1) as usize];
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [target.to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}
