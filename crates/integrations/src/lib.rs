//! `repoctx-integrations` — per-agent install machinery.
//!
//! M1.5 scope: manifest schema + parser only. Fetcher, installer, and
//! per-agent content land in sibling issues. The agent name set is the
//! contract — adding an agent is an additive change; removing is breaking.

mod error;
mod manifest;

pub use error::{IntegrationsError, Result};
pub use manifest::{Agent, File, Mode};

/// Agents available to the `repoctx hook` family. Order is the display
/// order used by `hook list`. Adding a new agent here is the additive
/// path; removing one is breaking.
pub const AGENTS: &[&str] = &["claude", "codex", "opencode"];
