//! Cross-scope `PreToolUse → Bash` hook scan + classification.
//!
//! Claude Code merges hooks across user-global + project + project-local
//! scopes and runs same-matcher hooks in parallel with non-deterministic
//! `updatedInput`. So before `repoctx init` writes its entry it must
//! refuse any configuration that would race: a foreign (non-allowlisted)
//! hook anywhere, or a repoctx/rtk hook in a scope that would double-fire
//! with the install target. See `wiki/decisions/2026-06-13-repoctx-init.md`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde_json::Value;

/// Install / scan scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    UserGlobal,
    Project,
    ProjectLocal,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::UserGlobal => "user-global (~/.claude/settings.json)",
            Self::Project => "project (.claude/settings.json)",
            Self::ProjectLocal => "project-local (.claude/settings.local.json)",
        }
    }
}

/// Which tool owns a hook entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    Repoctx,
    Rtk,
    /// Anything not on the allowlist — would race with repoctx.
    Foreign,
}

/// One discovered `PreToolUse → Bash` hook.
#[derive(Debug, Clone)]
pub struct ScopedHook {
    pub scope: Scope,
    pub command: String,
    pub kind: HookKind,
}

/// Classify a hook `command`. First token decides; a script path is
/// resolved (relative against `base`) and its `# <tool>-hook-version`
/// marker read. Wrappers (`bash -c …`) and anything unrecognized are
/// Foreign — conservative, so we never silently coexist with an unknown
/// rewriter.
pub fn classify(command: &str, base: &Path) -> HookKind {
    let Some(first) = command.split_whitespace().next() else {
        return HookKind::Foreign;
    };
    match first {
        "repoctx" => return HookKind::Repoctx,
        "rtk" => return HookKind::Rtk,
        // shells / wrappers hide the real program — don't guess.
        "bash" | "sh" | "zsh" | "env" | "exec" => return HookKind::Foreign,
        _ => {}
    }
    // A path to a script → read its fingerprint marker.
    if first.contains('/') || first.ends_with(".sh") {
        let path = if Path::new(first).is_absolute() {
            PathBuf::from(first)
        } else {
            base.join(first)
        };
        if let Some(marker) = crate::hook_marker::read(&path) {
            return match marker.tool.as_str() {
                "repoctx" => HookKind::Repoctx,
                "rtk" => HookKind::Rtk,
                _ => HookKind::Foreign,
            };
        }
    }
    HookKind::Foreign
}

/// Scan all three scopes for `PreToolUse → Bash` hook commands.
pub fn scan(repo_root: &Path) -> Vec<ScopedHook> {
    let mut out = Vec::new();
    let home = home_dir();
    let targets: &[(Scope, Option<PathBuf>, PathBuf)] = &[
        (
            Scope::UserGlobal,
            home.as_ref().map(|h| h.join(".claude/settings.json")),
            home.clone().unwrap_or_else(|| PathBuf::from(".")),
        ),
        (
            Scope::Project,
            Some(repo_root.join(".claude/settings.json")),
            repo_root.to_path_buf(),
        ),
        (
            Scope::ProjectLocal,
            Some(repo_root.join(".claude/settings.local.json")),
            repo_root.to_path_buf(),
        ),
    ];
    for (scope, path, base) in targets {
        let Some(path) = path else { continue };
        for command in bash_commands(path) {
            let kind = classify(&command, base);
            out.push(ScopedHook {
                scope: *scope,
                command,
                kind,
            });
        }
    }
    out
}

/// Refuse an install that would race, unless `force`. Implements the
/// race-detection ruleset from the design doc.
pub fn pre_install_check(target: Scope, hooks: &[ScopedHook], force: bool) -> Result<()> {
    if force {
        return Ok(());
    }

    // 1. Foreign hooks anywhere block every install.
    let foreign: Vec<&ScopedHook> = hooks
        .iter()
        .filter(|h| h.kind == HookKind::Foreign)
        .collect();
    if !foreign.is_empty() {
        let mut msg = String::from(
            "refusing to install: unrecognized PreToolUse → Bash hook(s) would race with repoctx.\n",
        );
        for h in &foreign {
            msg.push_str(&format!("  [{}] {}\n", h.scope.label(), h.command));
        }
        msg.push_str(
            "repoctx chains only rtk for now. Resolve, then re-run:\n\
             \x20 - remove/disable the hook above, or\n\
             \x20 - re-run with --force to install anyway (accepts the race).",
        );
        bail!(msg);
    }

    // 2. repoctx/rtk scope races (the allowlisted-but-double-firing rows).
    match target {
        Scope::Project | Scope::ProjectLocal => {
            if let Some(h) = hooks.iter().find(|h| h.scope == Scope::UserGlobal) {
                match h.kind {
                    HookKind::Rtk => bail!(race_msg(
                        "a user-global rtk hook would race with a project-local install",
                        &["run `repoctx init -g` instead (recommended — repoctx chains rtk globally)",
                          "or uninstall the global rtk hook",
                          "or re-run with --force"],
                    )),
                    HookKind::Repoctx => bail!(race_msg(
                        "a user-global repoctx hook would double-fire with a project-local install",
                        &["run `repoctx hook doctor -g` to refresh the global install",
                          "or re-run with --force"],
                    )),
                    HookKind::Foreign => {}
                }
            }
        }
        Scope::UserGlobal => {
            if let Some(h) = hooks
                .iter()
                .find(|h| matches!(h.scope, Scope::Project | Scope::ProjectLocal) && h.kind == HookKind::Repoctx)
            {
                bail!(race_msg(
                    &format!(
                        "a {} repoctx hook would double-fire with a global install",
                        h.scope.label()
                    ),
                    &["remove the project-local install first", "or re-run with --force"],
                ));
            }
        }
    }
    Ok(())
}

fn race_msg(problem: &str, options: &[&str]) -> String {
    let mut s = format!("refusing to install: {problem}.\nOptions:\n");
    for o in options {
        s.push_str(&format!("  - {o}\n"));
    }
    s.trim_end().to_string()
}

/// Read `PreToolUse → Bash` hook command strings from one settings file.
fn bash_commands(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Some(arr) = root
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|p| p.as_array())
    else {
        return out;
    };
    for entry in arr {
        if entry.get("matcher").and_then(|m| m.as_str()) != Some("Bash") {
            continue;
        }
        let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
            continue;
        };
        for h in hooks {
            if let Some(cmd) = h.get("command").and_then(|c| c.as_str()) {
                out.push(cmd.to_string());
            }
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn classify_first_token() {
        let base = Path::new("/tmp");
        assert_eq!(classify("repoctx hook claude", base), HookKind::Repoctx);
        assert_eq!(classify("rtk hook claude", base), HookKind::Rtk);
        assert_eq!(classify("my-tool --x", base), HookKind::Foreign);
        assert_eq!(classify("bash -c 'rtk hook claude'", base), HookKind::Foreign);
    }

    #[test]
    fn classify_script_via_marker() {
        let dir = TempDir::new().unwrap();
        let script = dir.path().join("hook.sh");
        std::fs::write(&script, "#!/usr/bin/env bash\n# repoctx-hook-version: 1\n").unwrap();
        assert_eq!(classify(script.to_str().unwrap(), dir.path()), HookKind::Repoctx);

        let rtk = dir.path().join("rtk-rewrite.sh");
        std::fs::write(&rtk, "#!/bin/sh\n# rtk-hook-version: 3\n").unwrap();
        assert_eq!(classify(rtk.to_str().unwrap(), dir.path()), HookKind::Rtk);

        let foreign = dir.path().join("other.sh");
        std::fs::write(&foreign, "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(classify(foreign.to_str().unwrap(), dir.path()), HookKind::Foreign);
    }

    #[test]
    fn classify_relative_script_against_base() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".repoctx")).unwrap();
        std::fs::write(
            dir.path().join(".repoctx/hook.sh"),
            "#!/usr/bin/env bash\n# repoctx-hook-version: 1\n",
        )
        .unwrap();
        assert_eq!(classify(".repoctx/hook.sh", dir.path()), HookKind::Repoctx);
    }

    fn hook(scope: Scope, kind: HookKind) -> ScopedHook {
        ScopedHook {
            scope,
            command: "x".into(),
            kind,
        }
    }

    #[test]
    fn foreign_blocks_unless_forced() {
        let hooks = vec![hook(Scope::Project, HookKind::Foreign)];
        assert!(pre_install_check(Scope::Project, &hooks, false).is_err());
        assert!(pre_install_check(Scope::Project, &hooks, true).is_ok());
    }

    #[test]
    fn global_rtk_blocks_local_install() {
        let hooks = vec![hook(Scope::UserGlobal, HookKind::Rtk)];
        let err = pre_install_check(Scope::Project, &hooks, false).unwrap_err();
        assert!(err.to_string().contains("init -g"));
    }

    #[test]
    fn local_repoctx_blocks_global_install() {
        let hooks = vec![hook(Scope::Project, HookKind::Repoctx)];
        assert!(pre_install_check(Scope::UserGlobal, &hooks, false).is_err());
    }

    #[test]
    fn same_scope_repoctx_is_allowed() {
        // A prior project repoctx hook + a project install = idempotent.
        let hooks = vec![hook(Scope::Project, HookKind::Repoctx)];
        assert!(pre_install_check(Scope::Project, &hooks, false).is_ok());
    }

    #[test]
    fn clean_slate_is_allowed() {
        assert!(pre_install_check(Scope::Project, &[], false).is_ok());
    }
}
