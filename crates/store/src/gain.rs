//! Gain analytics — usage records + aggregates.
//!
//! Privacy: the `usage` table stores aggregates only. Filenames are touched
//! transiently at record time (to sum `files.size` for the candidate paths)
//! but never persisted. `query` is NULL unless the caller opted in via
//! `--record-query`. Epic 4dd57c8.

use rusqlite::params;

use crate::error::Result;
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub ts_unix_ns: i64,
    pub command: String,
    pub candidate_files: u32,
    pub candidate_bytes: i64,
    pub estimated_baseline_tokens: i64,
    pub returned_tokens: i64,
    pub output_format: String,
    /// Only set when the caller opts in with `--record-query`.
    pub query: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GainTotals {
    pub invocations: u64,
    pub candidate_bytes: i64,
    pub estimated_baseline_tokens: i64,
    pub returned_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandBreakdown {
    pub command: String,
    pub invocations: u64,
    pub candidate_bytes: i64,
    pub estimated_baseline_tokens: i64,
    pub returned_tokens: i64,
}

impl Store {
    /// Append one usage row.
    pub fn record_usage(&mut self, r: &UsageRecord) -> Result<()> {
        self.conn().execute(
            "INSERT INTO usage(ts_unix_ns, command, candidate_files, candidate_bytes,
                               estimated_baseline_tokens, returned_tokens, output_format, query)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                r.ts_unix_ns,
                r.command,
                r.candidate_files,
                r.candidate_bytes,
                r.estimated_baseline_tokens,
                r.returned_tokens,
                r.output_format,
                r.query,
            ],
        )?;
        Ok(())
    }

    /// Aggregate totals across the window. `since_ns = None` => unbounded.
    pub fn gain_totals(&self, since_ns: Option<i64>) -> Result<GainTotals> {
        let row = self.conn().query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(candidate_bytes), 0),
                    COALESCE(SUM(estimated_baseline_tokens), 0),
                    COALESCE(SUM(returned_tokens), 0)
             FROM usage
             WHERE (?1 IS NULL OR ts_unix_ns >= ?1)",
            params![since_ns],
            |r| {
                Ok(GainTotals {
                    invocations: r.get(0)?,
                    candidate_bytes: r.get(1)?,
                    estimated_baseline_tokens: r.get(2)?,
                    returned_tokens: r.get(3)?,
                })
            },
        )?;
        Ok(row)
    }

    /// Per-command breakdown across the window. Deterministic order: command ASC.
    pub fn gain_per_command(&self, since_ns: Option<i64>) -> Result<Vec<CommandBreakdown>> {
        let mut stmt = self.conn().prepare(
            "SELECT command,
                    COUNT(*),
                    COALESCE(SUM(candidate_bytes), 0),
                    COALESCE(SUM(estimated_baseline_tokens), 0),
                    COALESCE(SUM(returned_tokens), 0)
             FROM usage
             WHERE (?1 IS NULL OR ts_unix_ns >= ?1)
             GROUP BY command
             ORDER BY command ASC",
        )?;
        let rows = stmt.query_map(params![since_ns], |r| {
            Ok(CommandBreakdown {
                command: r.get(0)?,
                invocations: r.get(1)?,
                candidate_bytes: r.get(2)?,
                estimated_baseline_tokens: r.get(3)?,
                returned_tokens: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Most-recent rows, newest first.
    pub fn gain_recent(&self, limit: usize) -> Result<Vec<UsageRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT ts_unix_ns, command, candidate_files, candidate_bytes,
                    estimated_baseline_tokens, returned_tokens, output_format, query
             FROM usage
             ORDER BY ts_unix_ns DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(UsageRecord {
                ts_unix_ns: r.get(0)?,
                command: r.get(1)?,
                candidate_files: r.get(2)?,
                candidate_bytes: r.get(3)?,
                estimated_baseline_tokens: r.get(4)?,
                returned_tokens: r.get(5)?,
                output_format: r.get(6)?,
                query: r.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// `(candidate_files, candidate_bytes)` summed across the supplied
    /// paths. Paths not present in `files` are silently skipped, so a
    /// caller's de-duplicated path list maps cleanly to a baseline count
    /// even when the index has been rebuilt mid-flight.
    pub fn gain_baseline_for_files(&self, paths: &[String]) -> Result<(u32, i64)> {
        if paths.is_empty() {
            return Ok((0, 0));
        }
        let mut count: u32 = 0;
        let mut total: i64 = 0;
        for chunk in paths.chunks(256) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM files WHERE path IN ({placeholders})"
            );
            let row =
                self.conn()
                    .query_row(&sql, rusqlite::params_from_iter(chunk.iter()), |r| {
                        Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)?))
                    })?;
            count += row.0;
            total += row.1;
        }
        Ok((count, total))
    }
}
