//! Shared filesystem walker for `index` and `status`.
//!
//! Skip rules per epic contract: ignore::WalkBuilder with hidden(false),
//! git_ignore on, follow_links(false), always skip `.git` and `.repoctx`,
//! skip files > 2 MiB.

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

/// Walker yielding candidates that index_cmd is willing to parse:
/// size cap enforced, non-UTF-8 files dropped with a warning.
pub fn collect_indexable(repo_root: &Path) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for c in iter(repo_root) {
        // Materialize once; re-stat avoided.
        if !is_utf8(&c.abs) {
            warn!(path = %c.abs.display(), "skipping non-UTF-8 file");
            continue;
        }
        out.push(c);
    }
    Ok(out)
}

/// Cheaper variant for `status` staleness counting: same skip rules but
/// without the UTF-8 read (just stat + extension). The (mtime_ns, size)
/// tuple is the invalidation key, and that is all status compares.
pub fn collect_stat(repo_root: &Path) -> Result<Vec<Candidate>> {
    Ok(iter(repo_root).collect())
}

fn iter(repo_root: &Path) -> impl Iterator<Item = Candidate> + use<'_> {
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

    walker.flatten().filter_map(move |dent| {
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            return None;
        }
        let abs = dent.path().to_path_buf();
        let language = Language::from_path(&abs)?;
        let rel = abs.strip_prefix(repo_root).ok()?;
        let rel_db = repoctx_store::to_db_path(rel);
        let meta = match dent.metadata() {
            Ok(m) => m,
            Err(e) => {
                debug!(path = %abs.display(), error = %e, "stat failed");
                return None;
            }
        };
        let size = meta.len();
        if size > MAX_FILE_BYTES {
            warn!(path = %abs.display(), size, "skipping file > 2 MiB");
            return None;
        }
        let mtime_ns = mtime_to_ns(&meta);
        Some(Candidate {
            abs,
            rel: rel_db,
            language,
            mtime_ns,
            size: size as i64,
        })
    })
}

fn mtime_to_ns(meta: &std::fs::Metadata) -> i64 {
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
