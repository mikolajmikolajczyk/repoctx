//! Shared helpers for store-reading commands (`symbols`, `status`,
//! `outline`, `definition`, `context`).

use std::path::Path;

use anyhow::{bail, Result};

/// Reject reads against a repo without an index. The contract message is
/// the same across every read command (epic e408787).
pub fn ensure_indexed(repo_root: &Path) -> Result<()> {
    if !repo_root.join(".repoctx/index.db").exists() {
        bail!("no index found — run 'repoctx index'");
    }
    Ok(())
}
