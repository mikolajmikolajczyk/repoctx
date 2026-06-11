//! Manifest schema for per-agent integrations.
//!
//! ```toml
//! name = "claude"
//! description = "Claude Code skill + CLAUDE.md guidance"
//!
//! [[file]]
//! src  = "SKILL.md"               # relative to manifest, may escape via ../shared/
//! dest = ".claude/skills/repoctx/SKILL.md"
//! mode = "write"                  # write | append | merge-section
//! start_marker = "<!-- repoctx:start -->"   # required for append / merge-section
//! end_marker   = "<!-- repoctx:end -->"     # required for merge-section only
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{IntegrationsError, Result};

/// One agent's installation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub description: String,
    #[serde(rename = "file", default)]
    pub files: Vec<File>,
}

/// One source-to-destination write rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    /// Relative path to the source file, anchored at the manifest's
    /// directory in the source tree. `../shared/...` is allowed so multiple
    /// agents can share a fragment.
    pub src: String,
    /// Relative destination inside the target repo. Must be relative; must
    /// not contain `..` segments.
    pub dest: String,
    pub mode: Mode,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_marker: Option<String>,
}

/// Write mode. Plain `String` rendering = lowercase kebab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Create or overwrite; differing existing content requires `--force`.
    Write,
    /// Append source to dest if `start_marker` is absent.
    Append,
    /// Replace block between markers, or append wrapped in markers.
    MergeSection,
}

impl Agent {
    /// Parse a manifest from TOML source. `path` is used in error messages.
    pub fn from_toml(text: &str, path: &Path) -> Result<Self> {
        let agent: Agent =
            toml::from_str(text).map_err(|source| IntegrationsError::ManifestParse {
                path: path.to_path_buf(),
                source,
            })?;
        agent.validate(path)?;
        Ok(agent)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.name.is_empty() {
            return Err(IntegrationsError::ManifestInvalid {
                path: path.to_path_buf(),
                reason: "name is empty".into(),
            });
        }
        if self.files.is_empty() {
            return Err(IntegrationsError::ManifestInvalid {
                path: path.to_path_buf(),
                reason: "no [[file]] entries".into(),
            });
        }
        for f in &self.files {
            f.validate(path)?;
        }
        Ok(())
    }
}

impl File {
    fn validate(&self, path: &Path) -> Result<()> {
        if self.src.is_empty() {
            return invalid(path, "file.src is empty");
        }
        if self.dest.is_empty() {
            return invalid(path, "file.dest is empty");
        }
        let dest = PathBuf::from(&self.dest);
        // Reject anything Windows or Unix would call absolute, plus a
        // leading `/` or `\` which is "rooted" on the foreign platform
        // (PathBuf::is_absolute on Windows says `/etc/x` is relative).
        if dest.is_absolute() || self.dest.starts_with('/') || self.dest.starts_with('\\') {
            return invalid(path, &format!("file.dest must be relative: {}", self.dest));
        }
        if dest
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return invalid(
                path,
                &format!("file.dest must not contain `..`: {}", self.dest),
            );
        }
        match self.mode {
            Mode::Write => {}
            Mode::Append => {
                if self.start_marker.is_none() {
                    return invalid(
                        path,
                        &format!("append mode requires start_marker (dest {})", self.dest),
                    );
                }
            }
            Mode::MergeSection => {
                if self.start_marker.is_none() || self.end_marker.is_none() {
                    return invalid(
                        path,
                        &format!(
                            "merge-section requires start_marker and end_marker (dest {})",
                            self.dest
                        ),
                    );
                }
            }
        }
        Ok(())
    }
}

fn invalid(path: &Path, reason: &str) -> Result<()> {
    Err(IntegrationsError::ManifestInvalid {
        path: path.to_path_buf(),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("manifest.toml")
    }

    #[test]
    fn parses_write_mode() {
        let toml = r#"
name = "claude"
description = "test"
[[file]]
src = "SKILL.md"
dest = ".claude/skills/repoctx/SKILL.md"
mode = "write"
"#;
        let a = Agent::from_toml(toml, &p()).unwrap();
        assert_eq!(a.name, "claude");
        assert_eq!(a.files.len(), 1);
        assert_eq!(a.files[0].mode, Mode::Write);
        assert!(a.files[0].start_marker.is_none());
    }

    #[test]
    fn parses_merge_section_with_markers() {
        let toml = r#"
name = "codex"
description = "test"
[[file]]
src = "../shared/AGENTS.md.fragment"
dest = "AGENTS.md"
mode = "merge-section"
start_marker = "<!-- repoctx:start -->"
end_marker = "<!-- repoctx:end -->"
"#;
        let a = Agent::from_toml(toml, &p()).unwrap();
        assert_eq!(a.files[0].mode, Mode::MergeSection);
        assert_eq!(
            a.files[0].start_marker.as_deref(),
            Some("<!-- repoctx:start -->")
        );
        assert_eq!(
            a.files[0].end_marker.as_deref(),
            Some("<!-- repoctx:end -->")
        );
    }

    #[test]
    fn parses_append_with_start_marker_only() {
        let toml = r###"
name = "x"
description = "test"
[[file]]
src = "frag.md"
dest = "OUT.md"
mode = "append"
start_marker = "## repoctx"
"###;
        let a = Agent::from_toml(toml, &p()).unwrap();
        assert_eq!(a.files[0].mode, Mode::Append);
    }

    #[test]
    fn rejects_merge_section_without_markers() {
        let toml = r#"
name = "x"
description = "test"
[[file]]
src = "a"
dest = "b"
mode = "merge-section"
"#;
        let err = Agent::from_toml(toml, &p()).unwrap_err();
        assert!(matches!(err, IntegrationsError::ManifestInvalid { .. }));
    }

    #[test]
    fn rejects_absolute_dest() {
        let toml = r#"
name = "x"
description = "test"
[[file]]
src = "a"
dest = "/etc/passwd"
mode = "write"
"#;
        let err = Agent::from_toml(toml, &p()).unwrap_err();
        assert!(matches!(err, IntegrationsError::ManifestInvalid { .. }));
    }

    #[test]
    fn rejects_parent_dir_in_dest() {
        let toml = r#"
name = "x"
description = "test"
[[file]]
src = "a"
dest = "../escape"
mode = "write"
"#;
        let err = Agent::from_toml(toml, &p()).unwrap_err();
        assert!(matches!(err, IntegrationsError::ManifestInvalid { .. }));
    }

    #[test]
    fn rejects_empty_files_table() {
        let toml = r#"
name = "x"
description = "test"
"#;
        let err = Agent::from_toml(toml, &p()).unwrap_err();
        assert!(matches!(err, IntegrationsError::ManifestInvalid { .. }));
    }

    #[test]
    fn src_may_escape_via_parent_dir() {
        // src is intentionally allowed to use ../ for ../shared/ fragments.
        let toml = r#"
name = "codex"
description = "test"
[[file]]
src = "../shared/AGENTS.md.fragment"
dest = "AGENTS.md"
mode = "merge-section"
start_marker = "a"
end_marker = "b"
"#;
        Agent::from_toml(toml, &p()).unwrap();
    }
}
