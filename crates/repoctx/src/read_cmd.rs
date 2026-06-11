//! Shared helpers for store-reading commands.
//!
//! Two entry points:
//!
//! - [`ensure_fresh`] — used by `symbols`, `outline`, `definition`,
//!   `context`. Runs an incremental `repoctx index` pass first so the
//!   answer reflects current disk state. Cheap on the no-op path (only
//!   files whose `(mtime_ns, size)` tuple differs are reparsed).
//! - [`ensure_db`] — used by `status` (which reports staleness by
//!   design — auto-reindexing would defeat the purpose) and `gain`
//!   (which queries the `usage` table only). Builds the DB from
//!   scratch on first run; never reindexes on top of an existing DB.

use std::path::Path;

use anyhow::Result;

/// Ensure the index is fresh w.r.t. the working tree. Runs an
/// incremental reindex. Quiet on stderr unless work happened.
pub fn ensure_fresh(repo_root: &Path) -> Result<()> {
    let db_existed = repo_root.join(".repoctx/index.db").exists();
    if !db_existed {
        eprintln!("no index found — indexing now...");
    }
    let summary = crate::index_cmd::run_silent(repo_root, false)?;
    if db_existed && (summary.indexed > 0 || summary.removed > 0) {
        eprintln!(
            "reindexed {} changed file(s), {} removed, in {} ms",
            summary.indexed, summary.removed, summary.duration_ms
        );
    }
    Ok(())
}

/// Ensure the DB exists. Does NOT run an incremental pass — callers
/// (`status`, `gain`) either want to observe staleness themselves or
/// don't care about symbol freshness.
pub fn ensure_db(repo_root: &Path) -> Result<()> {
    if repo_root.join(".repoctx/index.db").exists() {
        return Ok(());
    }
    eprintln!("no index found — indexing now...");
    crate::index_cmd::run_silent(repo_root, false)?;
    Ok(())
}
