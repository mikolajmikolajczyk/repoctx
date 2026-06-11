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
    Sqlite(rusqlite::Error),
}

/// Classify any rusqlite error using its sqlite error code. Busy / locked
/// after the configured busy_timeout becomes `Locked`; corruption /
/// not-a-database becomes `Corrupted`. Anything else falls through.
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::ffi::ErrorCode;
        if let rusqlite::Error::SqliteFailure(ref err, _) = e {
            match err.code {
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                    return StoreError::Corrupted(e);
                }
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                    return StoreError::Locked(e);
                }
                _ => {}
            }
        }
        StoreError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;
