//! Repository root resolution.
//!
//! Search-start = `--repo <path>` or `cwd`. Root = nearest ancestor (incl.
//! itself) containing a `.git` entry; if none, the search-start itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn resolve(repo_flag: Option<PathBuf>) -> Result<PathBuf> {
    let start = match repo_flag {
        Some(p) => p,
        None => std::env::current_dir().context("current_dir")?,
    };
    let abs = std::fs::canonicalize(&start)
        .with_context(|| format!("canonicalize {}", start.display()))?;
    Ok(find_repo_root(&abs))
}

fn find_repo_root(start: &Path) -> PathBuf {
    let mut p = start;
    loop {
        if p.join(".git").exists() {
            return p.to_path_buf();
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return start.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_git_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join(".git")).unwrap();
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_repo_root(&nested), root);
    }

    #[test]
    fn falls_back_to_start_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("no_git");
        fs::create_dir(&start).unwrap();
        assert_eq!(find_repo_root(&start), start);
    }
}
