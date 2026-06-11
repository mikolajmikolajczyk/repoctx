//! Shared helpers for store-reading commands (`symbols`, `status`,
//! `outline`, `definition`, `context`, `gain`).
//!
//! Two entry points:
//!
//! - [`ensure_fresh`] — used by `symbols`, `outline`, `definition`,
//!   `context`. Runs an incremental `repoctx index` pass first so the
//!   answer reflects current disk state. Cheap on the no-op path (only
//!   files whose `(mtime_ns, size)` tuple differs are reparsed).
//! - [`ensure_db`] — used by `status` (which reports staleness by
//!   design — auto-reindexing would defeat the purpose) and `gain`
//!   (which queries the `usage` table only). Just confirms the DB
//!   exists, indexing it from scratch if it doesn't.

use std::path::Path;

use anyhow::{bail, Result};

/// Ensure the index is fresh w.r.t. the working tree. Runs an
/// incremental reindex. Quiet on stderr unless work happened.
pub fn ensure_fresh(repo_root: &Path, no_auto_index: bool) -> Result<()> {
    let db_existed = repo_root.join(".repoctx/index.db").exists();
    if no_auto_index {
        if !db_existed {
            bail!("no index found — run 'repoctx index'");
        }
        return Ok(());
    }
    if !db_existed {
        eprintln!("no index found — indexing now (pass --no-auto-index to skip)...");
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
/// don't care about symbol freshness. Builds the DB from scratch on
/// first run when auto-index is enabled.
pub fn ensure_db(repo_root: &Path, no_auto_index: bool) -> Result<()> {
    if repo_root.join(".repoctx/index.db").exists() {
        return Ok(());
    }
    if no_auto_index {
        bail!("no index found — run 'repoctx index'");
    }
    eprintln!("no index found — indexing now (pass --no-auto-index to skip)...");
    crate::index_cmd::run_silent(repo_root, false)?;
    Ok(())
}
