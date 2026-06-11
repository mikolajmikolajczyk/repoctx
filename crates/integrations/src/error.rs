use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrationsError {
    #[error("manifest at {path} is not valid TOML: {source}")]
    ManifestParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("manifest at {path} is invalid: {reason}")]
    ManifestInvalid { path: PathBuf, reason: String },

    #[error("unknown agent: {0}. Known agents: {}", super::AGENTS.join(", "))]
    UnknownAgent(String),

    #[error("fetch failed: GET {url} ({message}). Cache miss at {}. Try --no-cache, --ref <git-ref>, or download manually.", cache_path.display())]
    Fetch {
        url: String,
        cache_path: PathBuf,
        message: String,
    },

    #[error("cache error at {}: {reason}", path.display())]
    Cache { path: PathBuf, reason: String },
}

pub type Result<T> = std::result::Result<T, IntegrationsError>;
