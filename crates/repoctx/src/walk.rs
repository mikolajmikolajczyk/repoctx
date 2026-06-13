//! Shared filesystem walker for `index` and `status`.
//!
//! Skip rules per epic contract: ignore::WalkBuilder with hidden(false),
//! git_ignore on, follow_links(false), always skip `.git` and `.repoctx`,
//! skip files > 2 MiB, skip non-UTF-8 files. Index logs warnings on each
//! skip; status counts silently — both see the same set of indexable files.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use ignore::WalkBuilder;
use repoctx_index::Language;
use tracing::{debug, warn};

pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct Candidate {
    pub abs: PathBuf,
    pub rel: String,
    pub language: Language,
    pub mtime_ns: i64,
    pub size: i64,
}

/// Walker used by `index`: emits a tracing warning per skipped file.
pub fn collect_indexable(repo_root: &Path) -> Result<Vec<Candidate>> {
    collect(repo_root, true)
}

/// Walker used by `status` and the auto-reindex helper: same skip rules,
/// but silent (no warnings on stderr). Keeps query stderr clean and
/// avoids re-emitting the same skip warning on every read command.
pub fn collect_quiet(repo_root: &Path) -> Result<Vec<Candidate>> {
    collect(repo_root, false)
}

/// Back-compat alias used by `status`.
pub fn collect_stat(repo_root: &Path) -> Result<Vec<Candidate>> {
    collect_quiet(repo_root)
}

fn collect(repo_root: &Path, warn_on_skip: bool) -> Result<Vec<Candidate>> {
    let walker = WalkBuilder::new(repo_root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| {
            let n = e.file_name();
            n != ".git" && n != ".repoctx"
        })
        .build();

    let mut out = Vec::new();
    for dent in walker.flatten() {
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let abs = dent.path().to_path_buf();
        let Some(language) = Language::from_path(&abs) else {
            continue;
        };
        let rel = match abs.strip_prefix(repo_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_db = repoctx_store::to_db_path(rel);
        let meta = match dent.metadata() {
            Ok(m) => m,
            Err(e) => {
                debug!(path = %abs.display(), error = %e, "stat failed");
                continue;
            }
        };
        let size = meta.len();
        if size > MAX_FILE_BYTES {
            if warn_on_skip {
                warn!(path = %abs.display(), size, "skipping file > 2 MiB");
            }
            continue;
        }
        if !is_utf8(&abs) {
            if warn_on_skip {
                warn!(path = %abs.display(), "skipping non-UTF-8 file");
            }
            continue;
        }
        let mtime_ns = mtime_to_ns(&meta);
        out.push(Candidate {
            abs,
            rel: rel_db,
            language,
            mtime_ns,
            size: size as i64,
        });
    }
    Ok(out)
}

fn mtime_to_ns(meta: &std::fs::Metadata) -> i64 {
    // Platforms without mtime support fall back to the epoch → the file
    // always looks "changed", so it's reparsed rather than skipped. Safe
    // (never stale), just not free.
    let m = meta.modified().unwrap_or(UNIX_EPOCH);
    match m.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos().min(i64::MAX as u128) as i64,
        Err(_) => 0,
    }
}

fn is_utf8(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => std::str::from_utf8(&bytes).is_ok(),
        Err(_) => false,
    }
}
