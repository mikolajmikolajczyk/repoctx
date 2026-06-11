use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    /// Capability is meaningful for some backends but the current one
    /// cannot answer it (e.g. position-based `definition` without LSP).
    #[error("backend does not support capability: {capability}")]
    Unsupported { capability: &'static str },

    #[error(transparent)]
    Store(#[from] repoctx_store::StoreError),

    #[error("backend error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BackendError>;
