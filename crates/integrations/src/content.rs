//! Embedded integration content.
//!
//! Per-agent manifests + their referenced fragments are compiled into the
//! binary via `include_str!`. There is no network path and no on-disk
//! cache: `hook install` works offline, airgapped, and at a pinned
//! version that always matches the running binary. The source files live
//! under `integrations/` at the repo root and ship *out* of the binary
//! only in the sense that they're the build-time inputs here.

use std::path::PathBuf;

use crate::error::{IntegrationsError, Result};
use crate::manifest::Agent;

const MANIFEST_FILE: &str = "manifest.toml";

/// Every file an agent manifest can reference, keyed by its path relative
/// to `integrations/`. Adding an agent or fragment means adding a row.
static FILES: &[(&str, &str)] = &[
    (
        "claude/manifest.toml",
        include_str!("../../../integrations/claude/manifest.toml"),
    ),
    (
        "claude/CLAUDE.md.fragment",
        include_str!("../../../integrations/claude/CLAUDE.md.fragment"),
    ),
    (
        "codex/manifest.toml",
        include_str!("../../../integrations/codex/manifest.toml"),
    ),
    (
        "opencode/manifest.toml",
        include_str!("../../../integrations/opencode/manifest.toml"),
    ),
    (
        "opencode/plugin.ts",
        include_str!("../../../integrations/opencode/plugin.ts"),
    ),
    (
        "shared/AGENTS.md.fragment",
        include_str!("../../../integrations/shared/AGENTS.md.fragment"),
    ),
    (
        "shared/SKILL.md",
        include_str!("../../../integrations/shared/SKILL.md"),
    ),
];

fn lookup(path: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, content)| *content)
}

/// Parse an agent's embedded manifest.
pub fn manifest(agent: &str) -> Result<Agent> {
    validate_agent_name(agent)?;
    let key = format!("{agent}/{MANIFEST_FILE}");
    let text = lookup(&key).ok_or_else(|| IntegrationsError::EmbeddedMissing(key.clone()))?;
    Agent::from_toml(text, &PathBuf::from(format!("integrations/{key}")))
}

/// Fetch one file referenced by a manifest. `src` is the manifest's
/// `file.src` value — may use `../shared/...` to reach the shared dir.
pub fn file(agent: &str, src: &str) -> Result<Vec<u8>> {
    validate_agent_name(agent)?;
    let key = resolve(agent, src)?;
    lookup(&key)
        .map(|s| s.as_bytes().to_vec())
        .ok_or(IntegrationsError::EmbeddedMissing(key))
}

fn validate_agent_name(agent: &str) -> Result<()> {
    if !crate::AGENTS.contains(&agent) {
        return Err(IntegrationsError::UnknownAgent(agent.to_string()));
    }
    Ok(())
}

/// Compose `<agent>/<rel>`, normalize `..`/`.`, and ensure the result
/// stays inside `integrations/`. Returns the lookup key.
fn resolve(agent: &str, rel: &str) -> Result<String> {
    let mut comps: Vec<String> = vec![agent.to_string()];
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        match seg {
            "." => {}
            ".." => {
                if comps.pop().is_none() {
                    return Err(IntegrationsError::ManifestInvalid {
                        path: PathBuf::from(rel),
                        reason: format!("src `{rel}` escapes the integrations/ root"),
                    });
                }
            }
            other => comps.push(other.to_string()),
        }
    }
    if comps.is_empty() {
        return Err(IntegrationsError::ManifestInvalid {
            path: PathBuf::from(rel),
            reason: format!("src `{rel}` resolves to empty path"),
        });
    }
    Ok(comps.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_manifest_parses_and_its_files_resolve() {
        for agent in crate::AGENTS {
            let m = manifest(agent).unwrap_or_else(|e| panic!("{agent} manifest: {e}"));
            assert_eq!(&m.name, agent);
            for f in &m.files {
                file(agent, &f.src).unwrap_or_else(|e| panic!("{agent} src `{}`: {e}", f.src));
            }
        }
    }

    #[test]
    fn shared_resolves_via_parent_dir() {
        // codex references ../shared/SKILL.md + ../shared/AGENTS.md.fragment
        let b = file("codex", "../shared/SKILL.md").unwrap();
        assert!(!b.is_empty());
    }

    #[test]
    fn unknown_agent_rejected() {
        assert!(matches!(
            manifest("aider"),
            Err(IntegrationsError::UnknownAgent(_))
        ));
    }

    #[test]
    fn src_escaping_root_rejected() {
        assert!(matches!(
            file("claude", "../../etc/passwd"),
            Err(IntegrationsError::ManifestInvalid { .. })
        ));
    }

    #[test]
    fn missing_embedded_file_errors() {
        assert!(matches!(
            file("claude", "does-not-exist.md"),
            Err(IntegrationsError::EmbeddedMissing(_))
        ));
    }
}
