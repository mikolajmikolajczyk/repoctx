//! `.claude/settings.json` PreToolUse-Bash ownership takeover.
//!
//! Called as a side-step after `repoctx hook install claude`. Reads
//! the existing settings file, captures any commands currently
//! registered under the Bash matcher, saves them into the
//! `hook.chain_commands` config key, then rewrites the file so that
//! `repoctx hook claude` is the only Bash hook left. At runtime the
//! hook handler chains through the saved commands on passthrough.
//!
//! Design: `wiki/decisions/2026-06-12-rewrite-hook-design.md`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::output::HumanRender;

/// What changed during a takeover pass. Returned for both the dry-run
/// preview and the live install.
#[derive(Debug, Clone, Serialize)]
pub struct TakeoverReport {
    pub settings_path: PathBuf,
    pub displaced_commands: Vec<String>,
    pub already_owned: bool,
    pub dry_run: bool,
}

impl HumanRender for TakeoverReport {
    fn human(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("settings: {}\n", self.settings_path.display()));
        if self.dry_run {
            s.push_str("mode:     dry-run\n");
        }
        if self.already_owned {
            s.push_str("status:   already owned by repoctx — no changes\n");
        } else if self.displaced_commands.is_empty() {
            s.push_str("status:   sole owner — no prior Bash hooks\n");
        } else {
            s.push_str(&format!(
                "status:   {} prior Bash hook(s) displaced; chained\n",
                self.displaced_commands.len()
            ));
            for cmd in &self.displaced_commands {
                s.push_str(&format!("  - {cmd}\n"));
            }
        }
        s.trim_end().to_string()
    }
}

const REPOCTX_HOOK_COMMAND: &str = "repoctx hook claude";
/// SessionStart hook: inject the repo orientation digest so the agent starts
/// primed to use repoctx (issue #11, adoption-via-priming). stdout becomes
/// session context. `2>/dev/null` keeps a stale/missing index from leaking a
/// hook error into the transcript — no digest is better than an error banner.
const REPOCTX_PRIME_COMMAND: &str = "repoctx prime 2>/dev/null";

/// Findings from a scan of `~/.claude/settings.json`. repoctx is
/// deliberately project-scoped — we never write to user-global
/// config — but we DO read it to detect a class of incompatibility
/// where a tool like rtk installed user-globally will fire in
/// parallel with our project-local entry.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UserGlobalScan {
    /// Commands found under `hooks.PreToolUse[].matcher == "Bash"`
    /// in `~/.claude/settings.json`. Empty if file absent or has no
    /// Bash entries. The repoctx command itself (if present user-
    /// globally for some reason) is excluded.
    pub conflicting_commands: Vec<String>,
}

impl HumanRender for UserGlobalScan {
    fn human(&self) -> String {
        if self.conflicting_commands.is_empty() {
            return "user-global: no conflicting hooks".into();
        }
        let mut s = String::from("user-global PreToolUse → Bash entries detected:");
        for cmd in &self.conflicting_commands {
            s.push_str(&format!("\n  - {cmd}"));
        }
        s
    }
}

/// Scan `~/.claude/settings.json` (or the resolved equivalent) for
/// Bash matcher entries. Read-only — we never write here.
pub fn scan_user_global() -> Result<UserGlobalScan> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(UserGlobalScan::default());
    };
    scan_user_global_at(&home)
}

/// Same as [`scan_user_global`] but against an explicit home dir — lets
/// tests inject a tempdir instead of mutating the process `HOME` env.
pub fn scan_user_global_at(home: &Path) -> Result<UserGlobalScan> {
    let path = home.join(".claude/settings.json");
    if !path.exists() {
        return Ok(UserGlobalScan::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(UserGlobalScan::default());
    }
    let root: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let mut out = UserGlobalScan::default();
    let Some(arr) = root
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|p| p.as_array())
    else {
        return Ok(out);
    };
    for entry in arr {
        let is_bash = entry
            .get("matcher")
            .and_then(|m| m.as_str())
            .map(|s| s == "Bash")
            .unwrap_or(false);
        if !is_bash {
            continue;
        }
        let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
            continue;
        };
        for h in hooks {
            if is_repoctx_hook(h) {
                continue;
            }
            if let Some(cmd) = h.get("command").and_then(|c| c.as_str()) {
                out.conflicting_commands.push(cmd.to_string());
            }
        }
    }
    Ok(out)
}

/// Emit the user-global incompatibility warning to stderr. Idempotent
/// — safe to call from both `hook install claude` and `hook doctor`.
pub fn warn_user_global(scan: &UserGlobalScan) {
    if scan.conflicting_commands.is_empty() {
        return;
    }
    eprintln!();
    eprintln!("warning: user-global PreToolUse hooks detected.");
    for cmd in &scan.conflicting_commands {
        eprintln!("  ~/.claude/settings.json → command: {cmd}");
    }
    eprintln!();
    eprintln!(
        "Claude Code merges user-global + project-local hooks at runtime,\n\
         so this command will fire in parallel with 'repoctx hook claude'.\n\
         The last-completing rewrite wins — non-deterministic, machine-load\n\
         dependent.\n\
         \n\
         Options:\n\
         \x20 1. Move it to project-local install (rerun without -g/--global).\n\
         \x20 2. Disable the user-global entry by hand in ~/.claude/settings.json.\n\
         \x20 3. Accept the race; rerun 'repoctx hook doctor' after any reinstall."
    );
}

/// Top-level entry. Reads or creates `<dir>/.claude/settings.json`,
/// captures any non-repoctx Bash PreToolUse commands, rewrites the
/// file so repoctx is the sole owner. Idempotent.
pub fn run(dir: &Path, dry_run: bool) -> Result<TakeoverReport> {
    let settings_path = dir.join(".claude/settings.json");
    let mut root: Value = if settings_path.exists() {
        let text = fs::read_to_string(&settings_path)
            .with_context(|| format!("read {}", settings_path.display()))?;
        if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("parse {}", settings_path.display()))?
        }
    } else {
        json!({})
    };

    let displaced = takeover(&mut root)?;
    ensure_session_start(&mut root)?;

    let already_owned = displaced.is_empty() && {
        let bash = bash_matcher_entry(&root);
        bash.is_some()
            && bash
                .and_then(|m| m.get("hooks"))
                .and_then(|h| h.as_array())
                .map(|arr| arr.len() == 1 && arr.iter().all(is_repoctx_hook))
                .unwrap_or(false)
    };

    if !dry_run {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let pretty = serde_json::to_string_pretty(&root)? + "\n";
        fs::write(&settings_path, pretty)
            .with_context(|| format!("write {}", settings_path.display()))?;
    }

    Ok(TakeoverReport {
        settings_path,
        displaced_commands: displaced,
        already_owned,
        dry_run,
    })
}

/// Make `command` the sole `PreToolUse → Bash` hook in `settings_path`,
/// preserving non-Bash matchers. Used by `repoctx init` to point the
/// Bash hook at the committed `.repoctx/hook.sh` script (script-based
/// world). Returns whether the file content changed. Creates the file +
/// parent dirs when absent.
pub fn set_sole_bash_hook(settings_path: &Path, command: &str, dry_run: bool) -> Result<bool> {
    let mut root = load_or_empty(settings_path)?;
    let before = serde_json::to_string(&root).unwrap_or_default();

    let hooks = root
        .as_object_mut()
        .context("settings.json root is not a JSON object")?
        .entry("hooks")
        .or_insert(json!({}));
    let pretooluse = hooks
        .as_object_mut()
        .context("settings.json `hooks` is not a JSON object")?
        .entry("PreToolUse")
        .or_insert(json!([]))
        .as_array_mut()
        .context("settings.json `hooks.PreToolUse` is not a JSON array")?;

    // Drop every Bash matcher entry, keep the rest, append one fresh.
    let mut kept: Vec<Value> = pretooluse
        .drain(..)
        .filter(|e| e.get("matcher").and_then(|m| m.as_str()) != Some("Bash"))
        .collect();
    kept.push(json!({
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": command }]
    }));
    *pretooluse = kept;

    let after = serde_json::to_string(&root).unwrap_or_default();
    let changed = before != after;

    if changed && !dry_run {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let pretty = serde_json::to_string_pretty(&root)? + "\n";
        fs::write(settings_path, pretty)
            .with_context(|| format!("write {}", settings_path.display()))?;
    }
    Ok(changed)
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

/// In-place mutation. Returns the commands displaced by this pass.
fn takeover(root: &mut Value) -> Result<Vec<String>> {
    let hooks = root
        .as_object_mut()
        .context("settings.json root is not a JSON object")?
        .entry("hooks")
        .or_insert(json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .context("settings.json `hooks` is not a JSON object")?;
    let pretooluse = hooks_obj
        .entry("PreToolUse")
        .or_insert(json!([]))
        .as_array_mut()
        .context("settings.json `hooks.PreToolUse` is not a JSON array")?;

    let mut displaced = Vec::new();
    let mut new_pretooluse: Vec<Value> = Vec::with_capacity(pretooluse.len() + 1);
    let mut inserted_repoctx_block = false;

    for entry in pretooluse.drain(..) {
        let is_bash = entry
            .get("matcher")
            .and_then(|m| m.as_str())
            .map(|s| s == "Bash")
            .unwrap_or(false);
        if !is_bash {
            new_pretooluse.push(entry);
            continue;
        }
        // Bash matcher — pluck out non-repoctx hooks, save them.
        let Some(hooks_arr) = entry.get("hooks").and_then(|h| h.as_array()) else {
            // Malformed — drop the entry. The takeover ends up
            // inserting our own clean entry below.
            continue;
        };
        for hook_entry in hooks_arr {
            if is_repoctx_hook(hook_entry) {
                continue;
            }
            if let Some(cmd) = hook_entry.get("command").and_then(|c| c.as_str()) {
                displaced.push(cmd.to_string());
            }
        }
        if !inserted_repoctx_block {
            new_pretooluse.push(repoctx_bash_entry());
            inserted_repoctx_block = true;
        }
    }

    if !inserted_repoctx_block {
        new_pretooluse.push(repoctx_bash_entry());
    }

    *pretooluse = new_pretooluse;
    Ok(displaced)
}

/// Ensure exactly one repoctx `SessionStart` hook running `repoctx prime`.
/// Idempotent; preserves any other SessionStart entries the user has.
fn ensure_session_start(root: &mut Value) -> Result<()> {
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
            "hooks": [{ "type": "command", "command": REPOCTX_PRIME_COMMAND }]
        }));
    }
    Ok(())
}

fn is_repoctx_prime(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.trim() == REPOCTX_PRIME_COMMAND)
        .unwrap_or(false)
}

fn repoctx_bash_entry() -> Value {
    json!({
        "matcher": "Bash",
        "hooks": [
            {
                "type": "command",
                "command": REPOCTX_HOOK_COMMAND
            }
        ]
    })
}

fn is_repoctx_hook(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.trim() == REPOCTX_HOOK_COMMAND)
        .unwrap_or(false)
}

fn bash_matcher_entry(root: &Value) -> Option<&Value> {
    root.get("hooks")?
        .get("PreToolUse")?
        .as_array()?
        .iter()
        .find(|e| {
            e.get("matcher")
                .and_then(|m| m.as_str())
                .map(|s| s == "Bash")
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_settings(dir: &Path, content: &str) {
        let path = dir.join(".claude/settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn read_settings(dir: &Path) -> Value {
        let path = dir.join(".claude/settings.json");
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn fresh_repo_no_settings_file_creates_one() {
        let tmp = tempdir().unwrap();
        let report = run(tmp.path(), false).unwrap();
        assert!(report.displaced_commands.is_empty());
        assert!(report.settings_path.ends_with(".claude/settings.json"));
        let v = read_settings(tmp.path());
        let bash = bash_matcher_entry(&v).unwrap();
        assert_eq!(
            bash["hooks"][0]["command"].as_str().unwrap(),
            REPOCTX_HOOK_COMMAND
        );
    }

    #[test]
    fn pre_existing_rtk_hook_is_displaced() {
        let tmp = tempdir().unwrap();
        write_settings(
            tmp.path(),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [{"type": "command", "command": "rtk hook claude"}]
                        }
                    ]
                }
            }"#,
        );
        let report = run(tmp.path(), false).unwrap();
        assert_eq!(report.displaced_commands, vec!["rtk hook claude"]);
        let v = read_settings(tmp.path());
        let bash = bash_matcher_entry(&v).unwrap();
        let hooks = bash["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["command"].as_str().unwrap(), REPOCTX_HOOK_COMMAND);
    }

    #[test]
    fn multiple_displaced_commands_captured_in_order() {
        let tmp = tempdir().unwrap();
        write_settings(
            tmp.path(),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [
                                {"type": "command", "command": "rtk hook claude"},
                                {"type": "command", "command": "/usr/local/bin/other-tool"}
                            ]
                        }
                    ]
                }
            }"#,
        );
        let report = run(tmp.path(), false).unwrap();
        assert_eq!(
            report.displaced_commands,
            vec!["rtk hook claude", "/usr/local/bin/other-tool"]
        );
    }

    #[test]
    fn non_bash_matchers_left_alone() {
        let tmp = tempdir().unwrap();
        write_settings(
            tmp.path(),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Read",
                            "hooks": [{"type": "command", "command": "snooper"}]
                        }
                    ]
                }
            }"#,
        );
        run(tmp.path(), false).unwrap();
        let v = read_settings(tmp.path());
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2); // original Read entry + repoctx Bash entry
        assert!(arr.iter().any(|e| e["matcher"].as_str() == Some("Read")));
        assert!(arr.iter().any(|e| e["matcher"].as_str() == Some("Bash")));
    }

    #[test]
    fn idempotent_when_already_owned() {
        let tmp = tempdir().unwrap();
        run(tmp.path(), false).unwrap();
        let report = run(tmp.path(), false).unwrap();
        assert!(report.displaced_commands.is_empty());
        assert!(report.already_owned);
    }

    #[test]
    fn dry_run_does_not_write() {
        let tmp = tempdir().unwrap();
        let report = run(tmp.path(), true).unwrap();
        assert!(report.dry_run);
        assert!(!tmp.path().join(".claude/settings.json").exists());
    }

    #[test]
    fn empty_settings_file_treated_as_empty_object() {
        let tmp = tempdir().unwrap();
        write_settings(tmp.path(), "");
        run(tmp.path(), false).unwrap();
        let v = read_settings(tmp.path());
        assert!(bash_matcher_entry(&v).is_some());
    }

    #[test]
    fn scan_user_global_returns_empty_when_no_home_file() {
        let tmp = tempdir().unwrap();
        let scan = scan_user_global_at(tmp.path()).unwrap();
        assert!(scan.conflicting_commands.is_empty());
    }

    #[test]
    fn scan_user_global_finds_rtk_style_entry() {
        let tmp = tempdir().unwrap();
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [{"type": "command", "command": "rtk hook claude"}]
                        }
                    ]
                }
            }"#,
        )
        .unwrap();
        let scan = scan_user_global_at(tmp.path()).unwrap();
        assert_eq!(scan.conflicting_commands, vec!["rtk hook claude"]);
    }

    #[test]
    fn scan_user_global_skips_self() {
        let tmp = tempdir().unwrap();
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [{"type": "command", "command": "repoctx hook claude"}]
                        }
                    ]
                }
            }"#,
        )
        .unwrap();
        let scan = scan_user_global_at(tmp.path()).unwrap();
        assert!(scan.conflicting_commands.is_empty());
    }

    #[test]
    fn existing_repoctx_hook_not_self_chained() {
        let tmp = tempdir().unwrap();
        write_settings(
            tmp.path(),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [
                                {"type": "command", "command": "repoctx hook claude"},
                                {"type": "command", "command": "rtk hook claude"}
                            ]
                        }
                    ]
                }
            }"#,
        );
        let report = run(tmp.path(), false).unwrap();
        assert_eq!(report.displaced_commands, vec!["rtk hook claude"]);
        let v = read_settings(tmp.path());
        let bash = bash_matcher_entry(&v).unwrap();
        let hooks = bash["hooks"].as_array().unwrap();
        // Should be single repoctx entry now.
        assert_eq!(hooks.len(), 1);
    }

    fn session_start_prime_count(v: &Value) -> usize {
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
    fn install_registers_session_start_prime() {
        let tmp = tempdir().unwrap();
        run(tmp.path(), false).unwrap();
        let v = read_settings(tmp.path());
        assert_eq!(session_start_prime_count(&v), 1, "one prime SessionStart hook");
    }

    #[test]
    fn session_start_prime_is_idempotent_and_preserves_others() {
        let tmp = tempdir().unwrap();
        write_settings(
            tmp.path(),
            r#"{
                "hooks": {
                    "SessionStart": [
                        {"hooks": [{"type": "command", "command": "echo hi"}]}
                    ]
                }
            }"#,
        );
        run(tmp.path(), false).unwrap();
        run(tmp.path(), false).unwrap(); // twice — must not duplicate
        let v = read_settings(tmp.path());
        assert_eq!(session_start_prime_count(&v), 1, "no duplicate prime hook");
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        // user's own SessionStart entry preserved.
        assert!(arr.iter().any(|e| {
            e.get("hooks")
                .and_then(|h| h.as_array())
                .map(|hs| hs.iter().any(|x| x.get("command").and_then(|c| c.as_str()) == Some("echo hi")))
                .unwrap_or(false)
        }));
    }
}
