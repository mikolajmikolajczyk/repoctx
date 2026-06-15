use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use tracing::debug;

use crate::error::{Result, StoreError};
use crate::like;
use crate::migrations;
use crate::record::{
    CallEdgeRow, CallRecord, FileRecord, HookEventStat, ImportEdgeRow, ImportRecord, SymbolRecord,
};

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
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

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
        let mut conn = Connection::open_with_flags(path, flags)?;
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

    /// Replace one file's record + its symbols atomically. The
    /// `DELETE FROM files` cascades to `symbols` and `calls`, so call edges
    /// for the file are cleared here; re-insert them with [`upsert_calls`]
    /// in the same indexing pass.
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

    /// Replace the call edges for one file. Old edges were already removed by
    /// [`upsert_file`]'s cascading delete; this inserts the freshly extracted
    /// ones. Call it right after `upsert_file` for the same file.
    pub fn upsert_calls(&mut self, file_path: &str, calls: &[CallRecord]) -> Result<()> {
        if calls.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO calls(file_path, caller_name, caller_start_line, callee_name, site_line, site_column, resolution)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for c in calls {
                debug_assert_eq!(c.file_path, file_path);
                stmt.execute(params![
                    c.file_path,
                    c.caller_name,
                    c.caller_start_line,
                    c.callee_name,
                    c.site_line,
                    c.site_column,
                    c.resolution,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replace the import edges for one file. Old edges were already removed
    /// by [`upsert_file`]'s cascading delete; this inserts the freshly
    /// extracted ones. Call it right after `upsert_file` for the same file.
    pub fn upsert_imports(&mut self, file_path: &str, imports: &[ImportRecord]) -> Result<()> {
        if imports.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO imports(file_path, module, site_line, site_column, resolution)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
            )?;
            for im in imports {
                debug_assert_eq!(im.file_path, file_path);
                stmt.execute(params![
                    im.file_path,
                    im.module,
                    im.site_line,
                    im.site_column,
                    im.resolution,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Direct dependencies of `file`: every module specifier it imports,
    /// deterministically ordered. `file` is a DB-relative path.
    pub fn deps_of(&self, file: &str) -> Result<Vec<ImportEdgeRow>> {
        self.import_edges(
            "i.file_path = ?1
             ORDER BY i.site_line ASC, i.site_column ASC, i.module ASC",
            file,
        )
    }

    /// Reverse dependencies: every file whose import specifier *contains*
    /// `module` as a substring (so `storage-idb` matches
    /// `@adapters/storage-idb`). Deterministically ordered.
    pub fn importers_of(&self, module: &str) -> Result<Vec<ImportEdgeRow>> {
        let pattern = format!("%{}%", crate::like::escape(module));
        self.import_edges(
            "i.module LIKE ?1 ESCAPE '\\'
             ORDER BY i.file_path ASC, i.site_line ASC, i.site_column ASC",
            &pattern,
        )
    }

    /// Shared query for `deps_of`/`importers_of`. `tail` is the WHERE body
    /// plus ORDER BY; `bind` is the single positional parameter.
    fn import_edges(&self, tail: &str, bind: &str) -> Result<Vec<ImportEdgeRow>> {
        let sql = format!(
            "SELECT i.file_path, i.module, i.site_line, i.site_column, i.resolution
             FROM imports i
             WHERE {tail}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([bind], |row| {
            Ok(ImportEdgeRow {
                file_path: row.get(0)?,
                module: row.get(1)?,
                site_line: row.get(2)?,
                site_column: row.get(3)?,
                resolution: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Record one hook passthrough telemetry event (issue #7). Aggregate
    /// only — `tool`/`idiom`/`outcome` are fixed enum-like strings, never the
    /// command body. Timestamp is stamped here.
    pub fn record_hook_event(&mut self, tool: &str, idiom: &str, outcome: &str) -> Result<()> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO hook_events(ts_unix_ns, tool, idiom, outcome) VALUES(?1, ?2, ?3, ?4)",
            params![ts, tool, idiom, outcome],
        )?;
        Ok(())
    }

    /// Aggregated hook telemetry for `repoctx discover`: count per
    /// `(idiom, outcome, tool)`, ordered so the biggest buckets surface
    /// first. `since` filters to events at or after a unix-ns timestamp.
    pub fn hook_event_stats(&self, since: Option<i64>) -> Result<Vec<HookEventStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, idiom, outcome, COUNT(*) AS n
             FROM hook_events
             WHERE (?1 IS NULL OR ts_unix_ns >= ?1)
             GROUP BY idiom, outcome, tool
             ORDER BY n DESC, idiom ASC, outcome ASC, tool ASC",
        )?;
        let rows = stmt.query_map(params![since], |row| {
            Ok(HookEventStat {
                tool: row.get(0)?,
                idiom: row.get(1)?,
                outcome: row.get(2)?,
                count: row.get::<_, i64>(3)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
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

    /// Read one settings row. Returns `None` if absent.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Write a settings row (insert or overwrite).
    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Remove a settings row. Returns the number of rows deleted (0 or 1).
    pub fn delete_setting(&mut self, key: &str) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?)
    }

    /// All settings rows ordered by key.
    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM settings ORDER BY key ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Whether a file row exists for `path`.
    pub fn file_exists(&self, path: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE path = ?1",
            params![path],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// `(mtime_ns, size)` for one file, if indexed.
    pub fn file_stat(&self, path: &str) -> Result<Option<(i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT mtime_ns, size FROM files WHERE path = ?1")?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
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

    /// Direct callers of `name`: every call edge whose callee name is `name`.
    /// The caller is the resolved enclosing symbol; the callee column carries
    /// the resolved target(s) of `name` (several rows when `name` is
    /// ambiguous, `None` only if `name` itself is unindexed). Edges across the
    /// whole repo, ordered deterministically.
    pub fn callers_of(&self, name: &str) -> Result<Vec<CallEdgeRow>> {
        self.call_edges("c.callee_name = ?1", name)
    }

    /// Direct callees of `name`: every call edge made from within a symbol
    /// named `name`. The callee is `Some` when its name resolves to a repo
    /// symbol and `None` when external/unresolved.
    pub fn callees_of(&self, name: &str) -> Result<Vec<CallEdgeRow>> {
        self.call_edges("c.caller_name = ?1", name)
    }

    /// Shared join for `callers_of`/`callees_of`. `where_clause` selects on
    /// `c.callee_name`/`c.caller_name`; `bind` is the symbol name.
    fn call_edges(&self, where_clause: &str, bind: &str) -> Result<Vec<CallEdgeRow>> {
        let sql = format!(
            "SELECT caller_s.file_path, caller_s.name, caller_s.kind,
                    caller_s.start_line, caller_s.start_column, caller_s.end_line, caller_s.end_column,
                    c.callee_name,
                    callee_s.file_path, callee_s.name, callee_s.kind,
                    callee_s.start_line, callee_s.start_column, callee_s.end_line, callee_s.end_column,
                    c.site_line, c.site_column, c.resolution
             FROM calls c
             JOIN symbols caller_s
               ON caller_s.file_path = c.file_path
              AND caller_s.name = c.caller_name
              AND caller_s.start_line = c.caller_start_line
             LEFT JOIN symbols callee_s
               ON callee_s.name = c.callee_name
             WHERE {where_clause}
             ORDER BY caller_s.file_path ASC, caller_s.start_line ASC,
                      c.site_line ASC, c.site_column ASC,
                      callee_s.file_path ASC, callee_s.start_line ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([bind], |row| {
            let callee = match row.get::<_, Option<String>>(8)? {
                Some(file_path) => Some(SymbolRecord {
                    file_path,
                    name: row.get(9)?,
                    kind: row.get(10)?,
                    start_line: row.get(11)?,
                    start_column: row.get(12)?,
                    end_line: row.get(13)?,
                    end_column: row.get(14)?,
                }),
                None => None,
            };
            Ok(CallEdgeRow {
                caller: SymbolRecord {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    start_line: row.get(3)?,
                    start_column: row.get(4)?,
                    end_line: row.get(5)?,
                    end_column: row.get(6)?,
                },
                callee_name: row.get(7)?,
                callee,
                site_line: row.get(15)?,
                site_column: row.get(16)?,
                resolution: row.get(17)?,
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

// Classification lives on `impl From<rusqlite::Error> for StoreError`, so
// every `?` on a rusqlite Result picks up the typed Locked/Corrupted
// variants automatically (including the initial open path below).
