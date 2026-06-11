use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("failed to create or access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("index is corrupted — delete .repoctx/ and re-run 'repoctx index'")]
    Corrupted(#[source] rusqlite::Error),

    #[error("index was created by a newer repoctx — upgrade repoctx or delete .repoctx/")]
    NewerSchema { db_version: u32, supported: u32 },

    #[error("index is locked by another repoctx process — retry")]
    Locked(#[source] rusqlite::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;
