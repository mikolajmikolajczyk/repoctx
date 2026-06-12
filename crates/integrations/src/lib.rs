//! `repoctx-integrations` — per-agent install machinery.
#![allow(clippy::result_large_err)] // Windows PathBuf inflates IntegrationsError; not hot-path.
//!
//! Per-agent manifests + fragments are embedded into the binary (see
//! [`content`]). `hook install` has no network path and no on-disk cache.
//! The agent name set is the contract — adding an agent is an additive
//! change; removing is breaking.

pub mod content;
mod error;
mod installer;
mod manifest;

pub use error::{IntegrationsError, Result};
pub use installer::{Action, InstallResult, Installer, WriteAction};
pub use manifest::{Agent, File, Mode};

/// Agents available to the `repoctx hook` family. Order is the display
/// order used by `hook list`. Adding a new agent here is the additive
/// path; removing one is breaking.
pub const AGENTS: &[&str] = &["claude", "codex", "opencode"];
