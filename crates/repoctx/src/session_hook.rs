//! Claude Code `SessionStart` hook management.
//!
//! repoctx's adoption path is **priming** (decision 2026-06-16-adoption-via-
//! priming): inject a repo orientation digest at session start. Rather than
//! wiring `repoctx prime` straight into `settings.json`, `repoctx init` writes a
//! shell script — `.claude/hooks/session-start.sh` — whose first step is
//! `repoctx prime`, and points the SessionStart hook at it. The script is
//! bashrc-style: a managed block (overwritten on re-init) plus a user region
//! the user can extend with their own session-start context. Everything the
//! script echoes to stdout lands in the agent's context.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Markers bounding the repoctx-managed region of the script. Content between
/// them is regenerated on every `repoctx init`; everything else is preserved.
const MANAGED_BEGIN: &str = "# >>> repoctx (managed — edits here are overwritten) >>>";
const MANAGED_END: &str = "# <<< repoctx (managed) <<<";

/// The managed block: run the orientation digest. `2>/dev/null` keeps a
/// stale/missing index from leaking a hook error into the transcript.
fn managed_block() -> String {
    format!("{MANAGED_BEGIN}\nrepoctx prime 2>/dev/null\n{MANAGED_END}")
}

/// Full script body for a fresh install: shebang, managed block, user region.
fn fresh_script() -> String {
    format!(
        "#!/usr/bin/env bash\n\
         # Claude Code SessionStart hook. Anything echoed to stdout here is\n\
         # injected into the agent's context at the start of every session.\n\
         #\n\
         # The block below is managed by `repoctx init` (regenerated on re-run).\n\
         # Add your own context after it — it is preserved across re-runs.\n\
         \n\
         {}\n\
         \n\
         # --- your session-start context below (preserved across `repoctx init`) ---\n\
         # e.g.  echo \"Reminder: ship via 'make release', never push to main.\"\n",
        managed_block()
    )
}

/// Insert/refresh the managed block in an existing custom script without
/// disturbing the user's own lines. If the markers are present, the region
/// between them is replaced; otherwise the block is inserted after the shebang
/// (or prepended).
fn merge_managed(existing: &str) -> String {
    if let (Some(b), Some(e)) = (existing.find(MANAGED_BEGIN), existing.find(MANAGED_END)) {
        if e > b {
            let end = e + MANAGED_END.len();
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..b]);
            out.push_str(&managed_block());
            out.push_str(&existing[end..]);
            return out;
        }
    }
    // No managed markers — keep the user's script, inject the block after the
    // shebang line (or at the top).
    let block = format!("{}\n", managed_block());
    if let Some(nl) = existing.find('\n') {
        if existing.starts_with("#!") {
            let (head, rest) = existing.split_at(nl + 1);
            return format!("{head}\n{block}{rest}");
        }
    }
    format!("{block}\n{existing}")
}

/// Write (or update) the SessionStart script, preserving any user content.
/// Returns whether the file changed.
pub fn write_script(script_path: &Path, dry_run: bool) -> Result<bool> {
    let next = match fs::read_to_string(script_path) {
        Ok(existing) => merge_managed(&existing),
        Err(_) => fresh_script(),
    };
    let changed = fs::read_to_string(script_path).map(|c| c != next).unwrap_or(true);
    if changed && !dry_run {
        if let Some(parent) = script_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(script_path, &next).with_context(|| format!("write {}", script_path.display()))?;
        make_executable(script_path);
    }
    Ok(changed)
}

/// Ensure one repoctx `SessionStart` hook in `settings_path` running `command`,
/// preserving other SessionStart entries. Idempotent. Returns whether changed.
pub fn install_settings(settings_path: &Path, command: &str, dry_run: bool) -> Result<bool> {
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
            .map(|hs| hs.iter().any(|h| is_our_hook(h, command)))
            .unwrap_or(false)
    });
    if !present {
        arr.push(json!({
            "hooks": [{ "type": "command", "command": command }]
        }));
    }

    let changed = serde_json::to_string(&root).unwrap_or_default() != before;
    if changed && !dry_run {
        write_json(settings_path, &root)?;
    }
    Ok(changed)
}

/// Remove repoctx's SessionStart entry (matching `command`) and strip the
/// managed block from the script — deleting the script if nothing but the
/// managed block + boilerplate remains. Returns whether the settings changed.
pub fn uninstall(settings_path: &Path, script_path: &Path, command: &str, dry_run: bool) -> Result<bool> {
    // Strip the managed block from the script; delete if no user content.
    if let Ok(existing) = fs::read_to_string(script_path) {
        if let (Some(b), Some(e)) = (existing.find(MANAGED_BEGIN), existing.find(MANAGED_END)) {
            if e > b && !dry_run {
                let end = e + MANAGED_END.len();
                let stripped = format!("{}{}", &existing[..b], &existing[end..]);
                if user_content_is_empty(&stripped) {
                    let _ = fs::remove_file(script_path);
                } else {
                    let _ = fs::write(script_path, stripped);
                }
            }
        }
    }

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
    arr.retain(|e| {
        let hooks = e.get("hooks").and_then(|h| h.as_array());
        match hooks {
            Some(hs) if !hs.is_empty() => !hs.iter().all(|h| is_our_hook(h, command)),
            _ => true,
        }
    });
    let changed = arr.len() != before;
    if changed && !dry_run {
        write_json(settings_path, &root)?;
    }
    Ok(changed)
}

fn is_our_hook(entry: &Value, command: &str) -> bool {
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.trim() == command || s.contains("session-start.sh"))
        .unwrap_or(false)
}

/// True if a script (after the managed block was removed) holds only the
/// shebang + comments/blanks — i.e. the user never added anything.
fn user_content_is_empty(s: &str) -> bool {
    s.lines()
        .map(str::trim)
        .all(|l| l.is_empty() || l.starts_with('#'))
}

fn make_executable(path: &Path) {
    let _ = std::process::Command::new("chmod").arg("+x").arg(path).status();
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

fn write_json(path: &Path, root: &Value) -> Result<()> {
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

    const CMD: &str = "bash .claude/hooks/session-start.sh";

    fn count_hooks(settings: &Path) -> usize {
        let v: Value = serde_json::from_str(&fs::read_to_string(settings).unwrap()).unwrap();
        v.get("hooks")
            .and_then(|h| h.get("SessionStart"))
            .and_then(|s| s.as_array())
            .map(|a| {
                a.iter()
                    .filter(|e| {
                        e.get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hs| hs.iter().any(|h| is_our_hook(h, CMD)))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn fresh_script_has_managed_block_and_user_region() {
        let s = fresh_script();
        assert!(s.contains(MANAGED_BEGIN) && s.contains(MANAGED_END));
        assert!(s.contains("repoctx prime 2>/dev/null"));
        assert!(s.contains("your session-start context below"));
    }

    #[test]
    fn merge_preserves_user_lines_and_refreshes_block() {
        let user = format!(
            "#!/usr/bin/env bash\n{}\nrepoctx prime 2>/dev/null\n{}\necho \"my own note\"\n",
            MANAGED_BEGIN, MANAGED_END
        );
        let merged = merge_managed(&user);
        assert!(merged.contains("echo \"my own note\""), "user line kept");
        assert_eq!(merged.matches(MANAGED_BEGIN).count(), 1, "single managed block");
    }

    #[test]
    fn merge_injects_block_into_marker_less_script() {
        let custom = "#!/usr/bin/env bash\necho hello\n";
        let merged = merge_managed(custom);
        assert!(merged.contains("echo hello"));
        assert!(merged.contains(MANAGED_BEGIN));
    }

    #[test]
    fn write_install_uninstall_round_trip() {
        let tmp = tempdir().unwrap();
        let script = tmp.path().join(".claude/hooks/session-start.sh");
        let settings = tmp.path().join(".claude/settings.json");
        write_script(&script, false).unwrap();
        install_settings(&settings, CMD, false).unwrap();
        assert!(script.exists());
        assert_eq!(count_hooks(&settings), 1);
        // idempotent
        install_settings(&settings, CMD, false).unwrap();
        assert_eq!(count_hooks(&settings), 1);
        // uninstall removes hook + script (no user content)
        uninstall(&settings, &script, CMD, false).unwrap();
        assert_eq!(count_hooks(&settings), 0);
        assert!(!script.exists(), "script deleted when only managed content");
    }

    #[test]
    fn install_merges_into_existing_settings() {
        // Existing settings.local.json with unrelated keys + another hook must
        // survive; repoctx's SessionStart entry is appended, not overwritten.
        let tmp = tempdir().unwrap();
        let settings = tmp.path().join(".claude/settings.local.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            r#"{
                "permissions": {"allow": ["Bash(ls:*)"]},
                "hooks": {
                    "PreToolUse": [{"matcher":"Bash","hooks":[{"type":"command","command":"my-own-hook"}]}],
                    "SessionStart": [{"hooks":[{"type":"command","command":"echo mine"}]}]
                }
            }"#,
        )
        .unwrap();
        install_settings(&settings, CMD, false).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        // Unrelated key preserved.
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
        // Sibling hook type preserved.
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "my-own-hook");
        // User's own SessionStart entry preserved + ours appended.
        let ss = v["hooks"]["SessionStart"].as_array().unwrap();
        assert!(ss.iter().any(|e| e["hooks"][0]["command"] == "echo mine"));
        assert_eq!(count_hooks(&settings), 1);
    }

    #[test]
    fn uninstall_keeps_script_with_user_content() {
        let tmp = tempdir().unwrap();
        let script = tmp.path().join(".claude/hooks/session-start.sh");
        let settings = tmp.path().join(".claude/settings.json");
        write_script(&script, false).unwrap();
        // user appends a line
        let mut body = fs::read_to_string(&script).unwrap();
        body.push_str("echo \"keep me\"\n");
        fs::write(&script, body).unwrap();
        install_settings(&settings, CMD, false).unwrap();
        uninstall(&settings, &script, CMD, false).unwrap();
        assert!(script.exists(), "script kept (has user content)");
        let body = fs::read_to_string(&script).unwrap();
        assert!(body.contains("keep me"));
        assert!(!body.contains(MANAGED_BEGIN), "managed block stripped");
    }
}
