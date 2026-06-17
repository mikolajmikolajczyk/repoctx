//! Claude Code `SessionStart` hook management.
//!
//! repoctx's adoption path is **priming**, not per-command interception
//! (decision 2026-06-16-adoption-via-priming): we register a single
//! SessionStart hook that runs `repoctx prime`, so the repo orientation digest
//! lands in the agent's context at session start. This module installs/removes
//! exactly that entry in a `settings.json`, preserving everything else.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// The SessionStart command. `2>/dev/null` keeps a stale/missing index from
/// leaking a hook error into the transcript — no digest beats an error banner.
pub const PRIME_COMMAND: &str = "repoctx prime 2>/dev/null";

/// Ensure exactly one repoctx `SessionStart` hook (running `repoctx prime`) in
/// `settings_path`, preserving any other SessionStart entries. Idempotent.
/// Returns whether the file changed. Creates the file + parents when absent.
pub fn install(settings_path: &Path, dry_run: bool) -> Result<bool> {
    let mut root = load_or_empty(settings_path)?;
    let before = serde_json::to_string(&root).unwrap_or_default();

    let arr = root
        .as_object_mut()
        .context("settings.json root is not a JSON object")?
        .entry("hooks")
        .or_insert(json!({}))
        .as_object_mut()
        .context("settings.json `hooks` is not a JSON object")?
        .entry("SessionStart")
        .or_insert(json!([]))
        .as_array_mut()
        .context("settings.json `hooks.SessionStart` is not a JSON array")?;
    let present = arr.iter().any(|e| {
        e.get("hooks")
            .and_then(|h| h.as_array())
            .map(|hs| hs.iter().any(is_repoctx_prime))
            .unwrap_or(false)
    });
    if !present {
        arr.push(json!({
            "hooks": [{ "type": "command", "command": PRIME_COMMAND }]
        }));
    }

    let changed = serde_json::to_string(&root).unwrap_or_default() != before;
    if changed && !dry_run {
        write(settings_path, &root)?;
    }
    Ok(changed)
}

/// Remove repoctx's SessionStart prime entries from `settings_path`, leaving
/// foreign SessionStart entries intact. Returns whether anything was removed.
pub fn uninstall(settings_path: &Path, dry_run: bool) -> Result<bool> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let mut root = load_or_empty(settings_path)?;
    let Some(arr) = root
        .get_mut("hooks")
        .and_then(|h| h.get_mut("SessionStart"))
        .and_then(|s| s.as_array_mut())
    else {
        return Ok(false);
    };
    let before = arr.len();
    // Drop a SessionStart entry iff every hook in it is ours.
    arr.retain(|e| {
        let hooks = e.get("hooks").and_then(|h| h.as_array());
        match hooks {
            Some(hs) if !hs.is_empty() => !hs.iter().all(is_repoctx_prime),
            _ => true,
        }
    });
    let changed = arr.len() != before;
    if changed && !dry_run {
        write(settings_path, &root)?;
    }
    Ok(changed)
}

fn is_repoctx_prime(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.trim() == PRIME_COMMAND)
        .unwrap_or(false)
}

fn load_or_empty(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn write(path: &Path, root: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let pretty = serde_json::to_string_pretty(root)? + "\n";
    fs::write(path, pretty).with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn read(p: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
    }
    fn prime_count(v: &Value) -> usize {
        v.get("hooks")
            .and_then(|h| h.get("SessionStart"))
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|e| {
                        e.get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hs| hs.iter().any(is_repoctx_prime))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn install_is_idempotent_and_preserves_others() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join(".claude/settings.json");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .unwrap();
        assert!(install(&p, false).unwrap());
        assert!(!install(&p, false).unwrap(), "second install is a no-op");
        let v = read(&p);
        assert_eq!(prime_count(&v), 1);
        // user's own entry preserved.
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == "echo hi"));
    }

    #[test]
    fn install_then_uninstall_round_trips() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        install(&p, false).unwrap();
        assert_eq!(prime_count(&read(&p)), 1);
        assert!(uninstall(&p, false).unwrap());
        assert_eq!(prime_count(&read(&p)), 0);
        assert!(!uninstall(&p, false).unwrap(), "second uninstall is a no-op");
    }
}
