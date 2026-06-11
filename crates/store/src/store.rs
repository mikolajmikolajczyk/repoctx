use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use tracing::debug;

use crate::error::{Result, StoreError};
use crate::like;
use crate::migrations;
use crate::record::{FileRecord, SymbolRecord};

const SQLITE_BUSY_TIMEOUT_MS: u32 = 5000;

#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub files: u64,
    pub symbols: u64,
    pub per_language: Vec<(String, u64)>,
}

#[derive(Debug, Default, Clone)]
pub struct SymbolFilter<'a> {
    pub kind: Option<&'a str>,
    pub language: Option<&'a str>,
    pub limit: Option<usize>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the per-repo index DB at `<repo_root>/.repoctx/index.db`.
    ///
    /// Applies pragmas (WAL, foreign_keys=ON, busy_timeout) and migrations.
    pub fn open(repo_root: &Path) -> Result<Self> {
        let dir = repo_root.join(".repoctx");
        fs::create_dir_all(&dir).map_err(|source| StoreError::Io {
            path: dir.clone(),
            source,
        })?;
        let path = dir.join("index.db");
        Self::open_at(&path)
    }

    /// Open the store at an explicit path (mainly for tests).
    pub fn open_at(path: &Path) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI;
        let mut conn = match Connection::open_with_flags(path, flags) {
            Ok(c) => c,
            Err(e) => return Err(classify_open_error(e)),
        };
        Self::configure(&mut conn)?;
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory store (tests only).
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&mut conn)?;
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    fn configure(conn: &mut Connection) -> Result<()> {
        conn.busy_timeout(std::time::Duration::from_millis(
            SQLITE_BUSY_TIMEOUT_MS as u64,
        ))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32> {
        migrations::read_version(&self.conn)
    }

    /// Replace one file's record + its symbols atomically.
    pub fn upsert_file(&mut self, file: &FileRecord, symbols: &[SymbolRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM files WHERE path = ?1", params![file.path])?;
        tx.execute(
            "INSERT INTO files(path, mtime_ns, size, language) VALUES(?1, ?2, ?3, ?4)",
            params![file.path, file.mtime_ns, file.size, file.language],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbols(file_path, name, kind, start_line, start_column, end_line, end_column)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for s in symbols {
                debug_assert_eq!(s.file_path, file.path);
                stmt.execute(params![
                    s.file_path,
                    s.name,
                    s.kind,
                    s.start_line,
                    s.start_column,
                    s.end_line,
                    s.end_column,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Map of `path -> (mtime_ns, size)` for every indexed file.
    pub fn file_mtimes(&self) -> Result<HashMap<String, (i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, mtime_ns, size FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?),
            ))
        })?;
        let mut out = HashMap::new();
        for r in rows {
            let (p, t) = r?;
            out.insert(p, t);
        }
        Ok(out)
    }

    /// Remove rows for files no longer present on disk.
    pub fn prune(&mut self, absent_paths: &[String]) -> Result<usize> {
        if absent_paths.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut total = 0usize;
        for chunk in absent_paths.chunks(256) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("DELETE FROM files WHERE path IN ({placeholders})");
            total += tx.execute(sql.as_str(), params_from_iter(chunk.iter()))?;
        }
        tx.commit()?;
        Ok(total)
    }

    /// Case-insensitive substring search. Returns `(symbol, language)` rows
    /// ordered by `(name, file_path, start_line)`.
    pub fn symbols_substring(
        &self,
        query: &str,
        filter: &SymbolFilter<'_>,
    ) -> Result<Vec<(SymbolRecord, String)>> {
        let pattern = format!("%{}%", like::escape(query));
        let mut sql = String::from(
            "SELECT s.file_path, s.name, s.kind, s.start_line, s.start_column,
                    s.end_line, s.end_column, f.language
             FROM symbols s
             JOIN files f ON f.path = s.file_path
             WHERE s.name LIKE ?1 ESCAPE '\\'",
        );
        let mut binds: Vec<String> = vec![pattern];
        if let Some(k) = filter.kind {
            sql.push_str(" AND s.kind = ?");
            sql.push_str(&(binds.len() + 1).to_string());
            binds.push(k.to_string());
        }
        if let Some(l) = filter.language {
            sql.push_str(" AND f.language = ?");
            sql.push_str(&(binds.len() + 1).to_string());
            binds.push(l.to_string());
        }
        sql.push_str(" ORDER BY s.name COLLATE NOCASE ASC, s.file_path ASC, s.start_line ASC");
        if let Some(n) = filter.limit {
            sql.push_str(" LIMIT ?");
            sql.push_str(&(binds.len() + 1).to_string());
            binds.push(n.to_string());
        }
        debug!(?sql, ?binds, "symbols_substring");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds.iter()), |row| {
            Ok((
                SymbolRecord {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    start_line: row.get(3)?,
                    start_column: row.get(4)?,
                    end_line: row.get(5)?,
                    end_column: row.get(6)?,
                },
                row.get::<_, String>(7)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Symbols in one file, ordered by `(start_line, start_column)`.
    pub fn symbols_by_file(&self, path: &str) -> Result<Vec<SymbolRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, name, kind, start_line, start_column, end_line, end_column
             FROM symbols
             WHERE file_path = ?1
             ORDER BY start_line ASC, start_column ASC",
        )?;
        let rows = stmt.query_map([path], |row| {
            Ok(SymbolRecord {
                file_path: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                start_line: row.get(3)?,
                start_column: row.get(4)?,
                end_line: row.get(5)?,
                end_column: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn counts(&self) -> Result<Counts> {
        let files: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let symbols: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
        let mut stmt = self.conn.prepare(
            "SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY language ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        let mut per_language = Vec::new();
        for r in rows {
            per_language.push(r?);
        }
        Ok(Counts {
            files,
            symbols,
            per_language,
        })
    }

    /// Path to the open DB (`:memory:` for in-memory).
    pub fn path(&self) -> Option<PathBuf> {
        self.conn.path().map(PathBuf::from)
    }
}

fn classify_open_error(e: rusqlite::Error) -> StoreError {
    use rusqlite::ffi::ErrorCode;
    if let rusqlite::Error::SqliteFailure(ref err, _) = e {
        match err.code {
            ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                return StoreError::Corrupted(e)
            }
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => return StoreError::Locked(e),
            _ => {}
        }
    }
    StoreError::Sqlite(e)
}
