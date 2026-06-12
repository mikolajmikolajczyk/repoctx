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

    #[error("integration content `{0}` is not embedded in this binary (build bug)")]
    EmbeddedMissing(String),

    #[error("refusing to write {}: {reason}", path.display())]
    WriteRefused { path: PathBuf, reason: String },

    #[error("io error at {}: {reason}", path.display())]
    Io { path: PathBuf, reason: String },
}

pub type Result<T> = std::result::Result<T, IntegrationsError>;
