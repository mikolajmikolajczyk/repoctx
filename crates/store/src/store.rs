use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use tracing::debug;

use crate::error::{Result, StoreError};
use crate::like;
use crate::migrations;
use crate::record::{
    CallEdgeRow, CallRecord, FileRecord, ImportEdgeRow, ImportRecord, LocatedEdge, SymbolRecord,
};

const SQLITE_BUSY_TIMEOUT_MS: u32 = 5000;

/// Builtin/host method names (`Array.push`, `Map.get`, …). Even with
/// receiver-awareness a method call to one of these binds to a repo `method` of
/// the same name when one happens to exist (e.g. a `Stack.push`) — a
/// method→method collision we can't resolve without receiver *types*. So a
/// method call (`x.push()`) to any of these resolves to nothing. Free calls
/// (`push()`) and method calls to other names are unaffected — strictly looser
/// than the old blanket name stop-list, which dropped these names everywhere.
const BUILTIN_METHOD_NAMES: &str = "'get','set','has','delete','push','pop','shift','unshift',\
    'map','filter','forEach','reduce','find','findIndex','some',\
    'every','join','split','slice','splice','concat','flat',\
    'keys','values','entries','on','off','once','emit','then',\
    'catch','finally','add','remove','clear','toString','valueOf',\
    'call','apply','bind','test','exec','match','replace',\
    'includes','indexOf','startsWith','endsWith','next',\
    'log','warn','error','info','debug'";

/// SQL predicate deciding whether symbol `{s}` may resolve call `{c}` as its
/// callee, given the call's receiver-awareness (#9). A receiver-value method
/// call (`obj.foo()`) resolves **only** to a `method` — never to a free
/// `function` of the same name, so `map.set()` no longer binds to `fn set`; and
/// a method call to a builtin name (`BUILTIN_METHOD_NAMES`) resolves to nothing
/// (the method→method collision needs receiver types). A free/path call
/// (`foo()`, `Type::foo()`) resolves to `function`/`method`/`macro`. Data/doc
/// symbols (`key`/`section`) and types never resolve. `{c}` must expose an
/// `is_method` column; `{s}` a `kind` column.
fn callee_match(c: &str, s: &str) -> String {
    format!(
        "(({c}.is_method = 1 AND {s}.kind = 'method' \
              AND {c}.callee_name NOT IN ({BUILTIN_METHOD_NAMES})) \
          OR ({c}.is_method = 0 AND {s}.kind IN ('function','method','macro')))"
    )
}

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
                "INSERT INTO symbols(file_path, name, kind, start_line, start_column, end_line, end_column, visibility)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                    s.visibility,
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
                "INSERT INTO calls(file_path, caller_name, caller_start_line, callee_name, site_line, site_column, resolution, is_method)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                    c.is_method as i64,
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

    /// `(path, size)` for every indexed file — for module/size aggregation
    /// in `repoctx overview` (issue #5).
    pub fn file_sizes(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare("SELECT path, size FROM files")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Symbol count per file (`file_path`, count). For `overview` module stats.
    /// Per-file symbol counts as `(file_path, total, code)`. `code` excludes
    /// data/doc symbols (markdown `section`, config `key`) so `overview` can
    /// rank modules by real code, not headings/keys (issue #9-D).
    pub fn symbol_counts_by_file(&self) -> Result<Vec<(String, u64, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, COUNT(*) AS total,
                    SUM(CASE WHEN kind NOT IN ('key','section') THEN 1 ELSE 0 END) AS code
             FROM symbols GROUP BY file_path",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Public API surface (issue #10): code symbols whose lexical visibility is
    /// `public`, as `(file_path, name, kind)`. Only meaningful for languages
    /// with a visibility extractor (Go/Rust/JS/TS); others are `unknown` and
    /// never surface here. Excludes data/doc kinds. `overview` groups these by
    /// directory into the per-module exported surface.
    pub fn public_symbols(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, name, kind FROM symbols
             WHERE visibility = 'public'
               AND kind NOT IN ('key','section','field','variable')
             ORDER BY file_path ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Entry-point symbols: functions/methods named `main`. Heuristic — for
    /// `overview` (issue #5).
    pub fn entry_points(&self) -> Result<Vec<SymbolRecord>> {
        // `main` functions/methods (Rust/Go/C/C++/Java CLI entry).
        let mut stmt = self.conn.prepare(
            "SELECT file_path, name, kind, start_line, start_column, end_line, end_column, visibility
             FROM symbols
             WHERE name = 'main' AND kind IN ('function', 'method')
             ORDER BY file_path ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SymbolRecord {
                file_path: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                start_line: row.get(3)?,
                start_column: row.get(4)?,
                end_line: row.get(5)?,
                end_column: row.get(6)?,
                visibility: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }

        // JS/TS web-app bootstraps: a `main`/`index` module is the bundler
        // entry even though it defines no `main` function. Detected by file
        // basename (Vite/webpack/Parcel convention). Synthesized as a
        // `kind: "entry"` record at line 1 so `overview`/`report` stop reporting
        // "none detected" for SPA codebases.
        let mut fstmt = self.conn.prepare(
            "SELECT DISTINCT path FROM files
             WHERE path LIKE '%/main.tsx' OR path = 'main.tsx'
                OR path LIKE '%/main.jsx' OR path = 'main.jsx'
                OR path LIKE '%/main.ts'  OR path = 'main.ts'
                OR path LIKE '%/main.js'  OR path = 'main.js'
                OR path LIKE '%/index.tsx' OR path = 'index.tsx'
                OR path LIKE '%/index.jsx' OR path = 'index.jsx'
             ORDER BY path ASC",
        )?;
        let froms = fstmt.query_map([], |row| row.get::<_, String>(0))?;
        for f in froms {
            let path = f?;
            let base = path.rsplit('/').next().unwrap_or(&path).to_string();
            out.push(SymbolRecord {
                file_path: path,
                name: base,
                kind: "entry".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 0,
                visibility: "public".to_string(),
            });
        }
        Ok(out)
    }

    /// Hotspots: the most-called in-repo symbols by incoming call-edge count.
    /// Returns `(name, callers, file_path, start_line)`. For `overview` (#5).
    ///
    /// Counts only names that resolve **unambiguously to a single callable
    /// definition** (issue #9): a name with several defs is a collision, not
    /// centrality, and host-method names (`get`/`set`/`push`) usually resolve
    /// to many or none. Resolution is restricted to callable kinds so a
    /// `.on()` call never binds to a YAML `key` named `on` (#9-C).
    pub fn hotspots(&self, limit: usize) -> Result<Vec<(String, u64, String, u32)>> {
        let lim = if limit == 0 { -1 } else { limit as i64 };
        // Count an incoming edge only when the callee resolves, receiver-aware,
        // to exactly ONE callable symbol (centrality, not name popularity): a
        // `.get()`/`.push()` with no repo method of that name resolves to zero
        // and drops out — replacing the old host-method stop-list (#9).
        let m = callee_match("c", "s");
        let sql = format!(
            "SELECT c.callee_name, COUNT(*) AS n,
                    (SELECT s.file_path FROM symbols s
                     WHERE s.name = c.callee_name AND s.kind IN ('function','method')
                     ORDER BY s.file_path, s.start_line LIMIT 1),
                    (SELECT s.start_line FROM symbols s
                     WHERE s.name = c.callee_name AND s.kind IN ('function','method')
                     ORDER BY s.file_path, s.start_line LIMIT 1)
             FROM calls c
             WHERE (SELECT COUNT(*) FROM symbols s
                      WHERE s.name = c.callee_name AND {m}) = 1
             GROUP BY c.callee_name
             ORDER BY n DESC, c.callee_name ASC
             LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![lim], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Every import edge as `(file_path, module)`, for building an in-memory
    /// module graph (issue #4 — import cycles / dependency map). Ordered.
    pub fn all_import_edges(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, module FROM imports ORDER BY file_path ASC, site_line ASC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Set of every indexed file path — the resolution target set for the
    /// module-graph resolver.
    pub fn all_file_paths(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = HashSet::new();
        for r in rows {
            out.insert(r?);
        }
        Ok(out)
    }

    /// Boundary crossings: import edges whose importing file contains `from`
    /// AND whose module specifier contains `to` (both substring). Answers
    /// "does layer `from` import `to`?" from the import graph (ADR-0011).
    pub fn boundary_crossings(&self, from: &str, to: &str) -> Result<Vec<ImportEdgeRow>> {
        let from_pat = format!("%{}%", crate::like::escape(from));
        let to_pat = format!("%{}%", crate::like::escape(to));
        let sql = "SELECT i.file_path, i.module, i.site_line, i.site_column, i.resolution
                   FROM imports i
                   WHERE i.file_path LIKE ?1 ESCAPE '\\' AND i.module LIKE ?2 ESCAPE '\\'
                   ORDER BY i.file_path ASC, i.site_line ASC, i.site_column ASC";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![from_pat, to_pat], |row| {
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

    /// All import edges whose importing file path contains `from` (substring).
    /// For alias-aware `boundary`: each is resolved specifier→file at query
    /// time (issue #8).
    pub fn imports_under(&self, from: &str) -> Result<Vec<ImportEdgeRow>> {
        let pattern = format!("%{}%", crate::like::escape(from));
        self.import_edges(
            "i.file_path LIKE ?1 ESCAPE '\\'
             ORDER BY i.file_path ASC, i.site_line ASC",
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
                    s.end_line, s.end_column, f.language, s.visibility
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
                    visibility: row.get(8)?,
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
            "SELECT file_path, name, kind, start_line, start_column, end_line, end_column, visibility
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
                visibility: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Dead-code candidates: function/method symbols whose name is never a
    /// callee in any call edge (issue #3). Name-based — a symbol called only
    /// dynamically, via a trait object, or from outside the indexed scope
    /// (public API, FFI) shows up here as a false positive, so callers treat
    /// it as a candidate list, not proof. `lang` filters by language slug.
    ///
    /// Filters out guaranteed false positives: exported/public symbols
    /// (issue #10 — public API, called across boundaries name-based analysis
    /// can't see), type-declaration files (`.d.ts`), and test files (issue #9).
    pub fn uncalled_symbols(&self, lang: Option<&str>) -> Result<Vec<SymbolRecord>> {
        let sql = "SELECT s.file_path, s.name, s.kind, s.start_line, s.start_column,
                          s.end_line, s.end_column, s.visibility
                   FROM symbols s
                   JOIN files f ON f.path = s.file_path
                   WHERE s.kind IN ('function', 'method')
                     AND (?1 IS NULL OR f.language = ?1)
                     -- Only languages with call-site extraction (core-8): in any
                     -- other language there are no call edges, so EVERY function
                     -- would look uncalled. (Must match index::language calls_query.)
                     AND f.language IN ('rust','python','javascript','typescript',
                                        'tsx','go','c','cpp','java')
                     -- Exported/public symbols are API, not dead (issue #10).
                     -- 'unknown' (no visibility signal yet) is kept — current
                     -- behavior for languages without extraction.
                     AND s.visibility <> 'public'
                     -- Constructors are invoked via `new`, never called by name.
                     AND s.name <> 'constructor'
                     AND s.name NOT IN (SELECT callee_name FROM calls)
                     AND f.path NOT LIKE '%.d.ts'
                     AND f.path NOT LIKE '%.test.%'
                     AND f.path NOT LIKE '%.spec.%'
                     AND f.path NOT LIKE '%\\_test.%' ESCAPE '\\'
                     AND f.path NOT LIKE '%/tests/%'
                     AND f.path NOT LIKE '%/test/%'
                     AND f.path NOT LIKE 'tests/%'
                     AND f.path NOT LIKE 'test/%'
                   ORDER BY s.file_path ASC, s.start_line ASC";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![lang], |row| {
            Ok(SymbolRecord {
                file_path: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                start_line: row.get(3)?,
                start_column: row.get(4)?,
                end_line: row.get(5)?,
                end_column: row.get(6)?,
                visibility: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// All call edges as `(caller_name, callee_name)` pairs where BOTH names
    /// resolve to indexed symbols (so cycle detection only walks in-repo
    /// edges). Deduplicated. For `repoctx cycles` (issue #3).
    pub fn resolved_edge_pairs(&self) -> Result<Vec<(String, String)>> {
        // Only edges whose callee resolves, receiver-aware, to exactly ONE
        // callable definition: unambiguous (single def) and the right kind for
        // the call (`obj.foo()` -> a `method`, never a free `function`).
        // Ambiguous fan-out + miscategorized callees poison cycle detection and
        // community structure (issues #9, #14).
        let m = callee_match("c", "s");
        let sql = format!(
            "SELECT DISTINCT c.caller_name, c.callee_name
                   FROM calls c
                   WHERE c.caller_name <> c.callee_name
                     AND (SELECT COUNT(*) FROM symbols s
                            WHERE s.name = c.callee_name AND {m}) = 1
                   ORDER BY c.caller_name ASC, c.callee_name ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Every call edge with both endpoints located by definition, for
    /// node-identity-correct graph building (god nodes / communities / report /
    /// export). The caller is keyed by its enclosing definition
    /// `(name, file, caller_start_line)`; the callee carries `callee_defs` (the
    /// count of code symbols sharing its name) so consumers can split
    /// definitions and style ambiguity. When `callee_defs == 1` the callee's
    /// location is in `callee_file`/`callee_line`; for ambiguous (`>1`) callees
    /// the location is unknown (`None`). External callees (`0` defs) are
    /// dropped. Deduped per `(caller-def, callee-name)`.
    pub fn located_edges(&self) -> Result<Vec<LocatedEdge>> {
        // The callee location subqueries are read only when callee_defs == 1
        // (where the single def is unambiguous); LIMIT 1 keeps them scalar for
        // the ambiguous case we discard anyway. Callees resolve receiver-aware
        // (#9): a `.set()` with no repo method `set` resolves to 0 defs and is
        // dropped, so the false `set` super-hub never forms. `is_method` is in
        // the GROUP BY so a free and a method call to the same name from one
        // caller stay distinct edges (they resolve to different kinds).
        let m = callee_match("c", "s");
        let sql = format!(
            "SELECT c.caller_name, c.file_path, c.caller_start_line, c.callee_name,
                          (SELECT COUNT(*) FROM symbols s
                             WHERE s.name = c.callee_name AND {m}) AS defs,
                          (SELECT s.file_path FROM symbols s
                             WHERE s.name = c.callee_name AND {m}
                             ORDER BY s.file_path, s.start_line LIMIT 1) AS cfile,
                          (SELECT s.start_line FROM symbols s
                             WHERE s.name = c.callee_name AND {m}
                             ORDER BY s.file_path, s.start_line LIMIT 1) AS cline
                   FROM calls c
                   WHERE c.caller_name <> c.callee_name
                   GROUP BY c.caller_name, c.file_path, c.caller_start_line, c.callee_name, c.is_method
                   HAVING defs >= 1
                   ORDER BY c.caller_name, c.file_path, c.caller_start_line, c.callee_name"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let defs: i64 = row.get(4)?;
            let defs = defs as usize;
            let (callee_file, callee_line) = if defs == 1 {
                (Some(row.get(5)?), Some(row.get::<_, u32>(6)?))
            } else {
                (None, None)
            };
            Ok(LocatedEdge {
                caller_name: row.get(0)?,
                caller_file: row.get(1)?,
                caller_line: row.get::<_, u32>(2)?,
                callee_name: row.get(3)?,
                callee_defs: defs,
                callee_file,
                callee_line,
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
        let callee_match = callee_match("c", "callee_s");
        let sql = format!(
            "SELECT caller_s.file_path, caller_s.name, caller_s.kind,
                    caller_s.start_line, caller_s.start_column, caller_s.end_line, caller_s.end_column,
                    c.callee_name,
                    callee_s.file_path, callee_s.name, callee_s.kind,
                    callee_s.start_line, callee_s.start_column, callee_s.end_line, callee_s.end_column,
                    c.site_line, c.site_column, c.resolution,
                    caller_s.visibility, callee_s.visibility
             FROM calls c
             JOIN symbols caller_s
               ON caller_s.file_path = c.file_path
              AND caller_s.name = c.caller_name
              AND caller_s.start_line = c.caller_start_line
             LEFT JOIN symbols callee_s
               ON callee_s.name = c.callee_name
              -- Receiver-aware resolution (#9): a method call (`obj.foo()`)
              -- binds only to a `method`; a free/path call to a
              -- function/method/macro; never to a data/doc `key`/`section`. A
              -- `.set()` with no repo method `set` resolves to NULL = external.
              AND {callee_match}
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
                    visibility: row.get::<_, Option<String>>(19)?.unwrap_or_default(),
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
                    visibility: row.get(18)?,
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
