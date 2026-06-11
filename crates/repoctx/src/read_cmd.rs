//! Shared helpers for store-reading commands (`symbols`, `status`,
//! `outline`, `definition`, `context`).

use std::path::Path;

use anyhow::{bail, Result};

/// Make sure the repo has an index before a read command runs.
///
/// Default behavior: silently run `repoctx index` if the DB is missing.
/// A short progress line goes to stderr so big repos don't appear hung.
///
/// `no_auto_index = true` preserves the original behavior: bail with the
/// `no index found — run 'repoctx index'` error. Scripts that probe for
/// the un-indexed state (e.g. CI step ordering) should pass that flag.
pub fn ensure_indexed(repo_root: &Path, no_auto_index: bool) -> Result<()> {
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
