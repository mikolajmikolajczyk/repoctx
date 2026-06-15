//! `repoctx init` + `repoctx hook doctor` — first-class onboarding.
//!
//! `init` generates the committed `.repoctx/hook.sh` (dumb-pipe script),
//! points Claude Code's `PreToolUse → Bash` hook at it, writes
//! `.gitattributes`, installs the agent guidance files, refuses races
//! (via `hook_scan`), and migrates v0.5.x installs. `-g` does the same at
//! user-global scope (displacing + chaining a prior rtk hook). `doctor`
//! checks for drift/tamper + scope conflicts and repairs with `--fix`.
//! `--uninstall` reverses an install. See
//! `wiki/decisions/2026-06-13-repoctx-init.md`.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use repoctx_integrations::Installer;
use repoctx_store::Store;

use crate::config::HookUseRtk;

pub struct InitOpts {
    pub global: bool,
    pub agent: String,
    pub rtk: HookUseRtk,
    pub yes: bool,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run(repo_root: &Path, opts: InitOpts) -> Result<()> {
    if opts.agent != "claude" {
        bail!(
            "unsupported agent '{}': `repoctx init` supports 'claude' today. \
             For codex/opencode rules use `repoctx hook install <agent>`.",
            opts.agent
        );
    }

    // Refuse races (foreign hooks anywhere; repoctx/rtk in a scope that
    // would double-fire with the target) before doing anything. --force
    // overrides. See hook_scan + the design doc's ruleset.
    let target_scope = if opts.global {
        crate::hook_scan::Scope::UserGlobal
    } else {
        crate::hook_scan::Scope::Project
    };
    let scan = crate::hook_scan::scan(repo_root);

    // A user-global repoctx hook already fires for this project, so a
    // project-local hook would double-fire. Rather than refuse the whole
    // command, drop into guidance-only mode: install the skill + CLAUDE.md
    // (which never race) and skip the redundant project hook. --force still
    // forces a full project install (accepting the double-fire). Global
    // installs and the foreign/rtk race cases keep the strict check.
    let global_repoctx_active = !opts.global
        && scan.iter().any(|h| {
            h.scope == crate::hook_scan::Scope::UserGlobal
                && h.kind == crate::hook_scan::HookKind::Repoctx
        });
    let guidance_only = global_repoctx_active && !opts.force;
    if !guidance_only {
        crate::hook_scan::pre_install_check(target_scope, &scan, opts.force)?;
    } else {
        eprintln!(
            "note: a user-global repoctx hook is already active for this project.\n      \
             Installing guidance only (skill + CLAUDE.md) — skipping a project-local\n      \
             hook that would double-fire. Re-run with --force to install one anyway."
        );
    }

    let rtk_present = crate::hook_rewrite::which("rtk").is_some();
    let mut rtk_chain = match opts.rtk {
        HookUseRtk::On => true,
        HookUseRtk::Off => false,
        HookUseRtk::Auto => rtk_present,
    };

    // v0.5.x → script-based migration (project scope). Old installs have a
    // `hook.chain_commands` row + an inline `repoctx hook claude` settings
    // entry (the latter is replaced automatically by set_sole_bash_hook).
    let migration = if !opts.global {
        detect_chain_commands(repo_root)
    } else {
        None
    };
    if let Some(m) = &migration {
        if m.has_rtk {
            rtk_chain = true; // port the rtk chain into RTK_CHAIN
        }
    }

    // Global install displacing a user-global rtk hook: chain it underneath
    // so rtk's savings survive (the no-degradation promise). `--rtk off`
    // opts out, with a loud warning.
    let displacing_global_rtk = opts.global
        && scan.iter().any(|h| {
            h.scope == crate::hook_scan::Scope::UserGlobal
                && h.kind == crate::hook_scan::HookKind::Rtk
        });
    if displacing_global_rtk {
        if matches!(opts.rtk, HookUseRtk::Off) {
            eprintln!(
                "warning: replacing the user-global rtk hook with --rtk off — \
                 rtk's token savings will be LOST. Re-run without --rtk off to chain it."
            );
        } else {
            rtk_chain = true;
        }
    }

    // Interactive confirmation only on a TTY without --yes.
    if !opts.yes && io::stdin().is_terminal() {
        if guidance_only {
            if !prompt_yes_no(
                "Install repoctx guidance for this project (global hook already active)?",
                true,
            )? {
                eprintln!("aborted.");
                return Ok(());
            }
        } else {
            if rtk_present {
                rtk_chain = prompt_yes_no(
                    "rtk detected — chain it underneath repoctx (no degradation)?",
                    rtk_chain,
                )?;
            }
            let scope = if opts.global {
                "user-global"
            } else {
                "this project"
            };
            if !prompt_yes_no(&format!("Install repoctx hook for {scope}?"), true)? {
                eprintln!("aborted.");
                return Ok(());
            }
        }
    }

    // Scope-dependent paths.
    let (settings_path, script_path, entry_command) = scope_paths(repo_root, opts.global)?;

    let version = env!("CARGO_PKG_VERSION");
    let script = crate::hook_script::render(rtk_chain, version, "repoctx");

    if opts.dry_run {
        eprintln!(
            "repoctx init (dry-run){}",
            if opts.global { " -g" } else { "" }
        );
        if guidance_only {
            eprintln!("  mode        : guidance-only (user-global repoctx hook already active)");
            eprintln!("  would install: claude SKILL.md + CLAUDE.md guidance");
            return Ok(());
        }
        eprintln!("  rtk chaining: {}", if rtk_chain { "on" } else { "off" });
        eprintln!("  would write : {}", script_path.display());
        eprintln!(
            "  would set   : {} → {entry_command}",
            settings_path.display()
        );
        if !opts.global {
            eprintln!(
                "  would write : {}/.gitattributes (*.sh text eol=lf)",
                repo_root.display()
            );
            eprintln!("  would install: claude SKILL.md + CLAUDE.md guidance");
        }
        return Ok(());
    }

    // Back up an existing global settings.json before taking it over, so
    // a displaced rtk (or other) entry can be restored by hand.
    let backup = if !guidance_only && opts.global && settings_path.exists() {
        Some(backup_file(&settings_path)?)
    } else {
        None
    };

    // Guidance-only skips every hook write — the global hook already fires.
    if !guidance_only {
        write_script(&script_path, &script)?;
        if !opts.global {
            ensure_gitattributes(repo_root)?;
        }
        crate::hook_takeover::set_sole_bash_hook(&settings_path, &entry_command, false)?;

        // Finish the v0.5.x migration: drop the now-superseded chain_commands
        // row (chaining lives in the script's RTK_CHAIN now).
        if let Some(m) = &migration {
            if let Ok(mut store) = Store::open(repo_root) {
                let _ = store.delete_setting("hook.chain_commands");
            }
            eprintln!("  migrated    : v0.5.x hook.chain_commands → script-based");
            if m.has_rtk {
                eprintln!("                (rtk chain ported into RTK_CHAIN=1)");
            }
            if !m.others.is_empty() {
                eprintln!(
                    "  warning     : these chain commands could not be auto-ported \
                     (re-add by hand if needed): {}",
                    m.others.join(", ")
                );
            }
        }
    }

    // Agent guidance files. Project scope installs the skill + repo-root
    // AGENTS.md fragment into the repo; global scope installs only the skill
    // into ~/.claude/skills/ (no project to hold an AGENTS.md).
    let guidance_dir = if opts.global {
        home_dir().context("cannot resolve home directory")?
    } else {
        repo_root.to_path_buf()
    };
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    Installer::new(guidance_dir.clone())
        .force(opts.force)
        .global(opts.global)
        .var("REPOCTX_BIN", "repoctx")
        .var("REPO_NAME", repo_name)
        .var("REPO_ROOT", repo_root.display().to_string())
        .install("claude")
        .context("install claude guidance files")?;

    eprintln!("repoctx init: done.");
    if guidance_only {
        eprintln!("  mode        : guidance-only (user-global repoctx hook already active)");
        eprintln!(
            "  skill       : {}",
            guidance_dir
                .join(".claude/skills/repoctx/SKILL.md")
                .display()
        );
        eprintln!("  hook        : using the user-global hook (~/.claude/repoctx-hook.sh)");
        eprintln!("                run `repoctx init --force` to add a project-local hook instead.");
        return Ok(());
    }
    eprintln!("  hook script : {}", script_path.display());
    eprintln!(
        "  settings    : {} → {entry_command}",
        settings_path.display()
    );
    eprintln!(
        "  skill       : {}",
        guidance_dir
            .join(".claude/skills/repoctx/SKILL.md")
            .display()
    );
    eprintln!("  rtk chaining: {}", if rtk_chain { "on" } else { "off" });
    if displacing_global_rtk && rtk_chain {
        eprintln!("  rtk         : displaced the user-global rtk hook; chained underneath (no degradation)");
    }
    if let Some(b) = &backup {
        eprintln!("  backup      : {} (restore by hand to undo)", b.display());
    }
    if !rtk_present && rtk_chain {
        eprintln!("  note: rtk not on PATH yet — install it to activate chaining.");
    }

    // Project-local install can still race a user-global tool; surface it
    // (read-only) the same way `hook install` does.
    if !opts.global {
        if let Ok(scan) = crate::hook_takeover::scan_user_global() {
            crate::hook_takeover::warn_user_global(&scan);
        }
    }
    Ok(())
}

/// `repoctx init --uninstall [-g] [--restore-backup] [--force] [--dry-run]`.
/// Removes repoctx's own Bash hook entry (foreign/rtk entries untouched)
/// and deletes the hook script when it's verifiably ours. Leaves the
/// index DB, config, and agent guidance files alone (prints a recipe).
pub fn run_uninstall(
    repo_root: &Path,
    global: bool,
    force: bool,
    dry_run: bool,
    restore_backup: bool,
) -> Result<()> {
    let (settings_path, script_path, entry_command) = scope_paths(repo_root, global)?;

    // Global --restore-backup: reinstate the newest backup wholesale.
    if global && restore_backup {
        let Some(backup) = newest_backup(&settings_path) else {
            bail!(
                "no settings backup found next to {}",
                settings_path.display()
            );
        };
        if dry_run {
            eprintln!(
                "would restore {} → {}",
                backup.display(),
                settings_path.display()
            );
            return Ok(());
        }
        std::fs::copy(&backup, &settings_path)
            .with_context(|| format!("restore {}", backup.display()))?;
        eprintln!(
            "restored {} from {}",
            settings_path.display(),
            backup.display()
        );
        return Ok(());
    }

    // Verify the script is ours before deleting it.
    let script_is_ours = crate::hook_marker::read(&script_path)
        .map(|m| m.tool == "repoctx")
        .unwrap_or(false);
    if script_path.exists() && !script_is_ours && !force {
        bail!(
            "{} is not a repoctx-generated script (no marker / different tool); \
             refusing to delete. Inspect it, or re-run with --force.",
            script_path.display()
        );
    }

    if dry_run {
        eprintln!(
            "repoctx init --uninstall (dry-run){}",
            if global { " -g" } else { "" }
        );
        eprintln!(
            "  would remove settings entry: {} → {entry_command}",
            settings_path.display()
        );
        if script_path.exists() {
            eprintln!("  would delete script: {}", script_path.display());
        }
        return Ok(());
    }

    let removed = remove_our_bash_entry(&settings_path, &entry_command)?;
    if script_path.exists() {
        std::fs::remove_file(&script_path)
            .with_context(|| format!("remove {}", script_path.display()))?;
    }

    eprintln!("repoctx init --uninstall: done.");
    if removed {
        eprintln!("  removed settings entry: {}", settings_path.display());
    } else {
        eprintln!(
            "  no repoctx settings entry found at {}",
            settings_path.display()
        );
    }
    eprintln!(
        "  deleted hook script (if present): {}",
        script_path.display()
    );
    if !global {
        eprintln!("  left alone: .repoctx/index.db + config, CLAUDE.md, .claude/skills/repoctx/.");
        eprintln!("  to remove guidance: delete the repoctx block in CLAUDE.md + rm .claude/skills/repoctx/SKILL.md");
        eprintln!("  to wipe the index + config: rm -rf .repoctx");
    } else {
        eprintln!("  left alone: ~/.claude/skills/repoctx/ (the global skill).");
        eprintln!("  to remove guidance: rm -rf ~/.claude/skills/repoctx");
    }
    Ok(())
}

/// Remove Bash PreToolUse entries whose command equals `entry_command`
/// (ours). Foreign/rtk entries are preserved. Empty Bash matchers are
/// pruned. Returns whether anything was removed.
fn remove_our_bash_entry(settings_path: &Path, entry_command: &str) -> Result<bool> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(settings_path)
        .with_context(|| format!("read {}", settings_path.display()))?;
    let mut root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let Some(arr) = root
        .get_mut("hooks")
        .and_then(|h| h.get_mut("PreToolUse"))
        .and_then(|p| p.as_array_mut())
    else {
        return Ok(false);
    };
    let before = arr.len();
    arr.retain(|entry| {
        if entry.get("matcher").and_then(|m| m.as_str()) != Some("Bash") {
            return true;
        }
        // Drop the Bash matcher iff its only hook is ours.
        let only_ours = entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|hs| {
                !hs.is_empty()
                    && hs
                        .iter()
                        .all(|h| h.get("command").and_then(|c| c.as_str()) == Some(entry_command))
            })
            .unwrap_or(false);
        !only_ours
    });
    let changed = arr.len() != before;
    if changed {
        let pretty = serde_json::to_string_pretty(&root)? + "\n";
        std::fs::write(settings_path, pretty)
            .with_context(|| format!("write {}", settings_path.display()))?;
    }
    Ok(changed)
}

/// Newest `<name>.repoctx-backup-<ts>` next to `path` (lexical max = most
/// recent, since the suffix is a fixed-width unix-secs stamp).
fn newest_backup(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let prefix = format!("{}.repoctx-backup-", path.file_name()?.to_str()?);
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .max()
}

/// (settings.json path, hook script path, settings-entry command) for a
/// scope. Project entry is the relative `.repoctx/hook.sh`; global is the
/// absolute script path.
fn scope_paths(repo_root: &Path, global: bool) -> Result<(PathBuf, PathBuf, String)> {
    if global {
        let home = home_dir().context("cannot resolve home directory")?;
        let script = home.join(".claude/repoctx-hook.sh");
        let entry = script.display().to_string();
        Ok((home.join(".claude/settings.json"), script, entry))
    } else {
        Ok((
            repo_root.join(".claude/settings.json"),
            repo_root.join(".repoctx/hook.sh"),
            ".repoctx/hook.sh".to_string(),
        ))
    }
}

/// Resolve whether chaining should be on, from config (project) or rtk
/// presence (global, which has no per-repo config).
fn resolve_rtk_chain(repo_root: &Path, global: bool) -> bool {
    if global {
        return crate::hook_rewrite::which("rtk").is_some();
    }
    if !repo_root.join(".repoctx/index.db").exists() {
        return crate::hook_rewrite::which("rtk").is_some();
    }
    let Ok(store) = Store::open(repo_root) else {
        return crate::hook_rewrite::which("rtk").is_some();
    };
    let cfg =
        crate::config::Config::load(&store).unwrap_or_else(|_| crate::config::Config::defaults());
    match cfg.hook.use_rtk {
        HookUseRtk::On => true,
        HookUseRtk::Off => false,
        HookUseRtk::Auto => cfg
            .hook
            .chainable
            .iter()
            .any(|t| crate::hook_rewrite::which(t).is_some()),
    }
}

/// Compare two rendered scripts ignoring the environment/config-driven
/// value lines (RTK_CHAIN / MIN_VERSION / REPOCTX), so the drift check
/// catches body/logic tampering or staleness, not a flipped RTK_CHAIN.
fn structural(script: &str) -> String {
    script
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("RTK_CHAIN=")
                && !t.starts_with("MIN_VERSION=")
                && !t.starts_with("REPOCTX=")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repoctx hook doctor [-g] [--fix]` — drift/tamper check + scope-conflict
/// report; `--fix` regenerates the script + restores the settings entry.
pub fn run_doctor(repo_root: &Path, global: bool, fix: bool) -> Result<()> {
    let (settings_path, script_path, entry_command) = scope_paths(repo_root, global)?;
    let rtk_chain = resolve_rtk_chain(repo_root, global);
    let version = env!("CARGO_PKG_VERSION");
    let expected = crate::hook_script::render(rtk_chain, version, "repoctx");

    let mut issues: Vec<String> = Vec::new();

    // 1. Script presence + structural drift.
    match std::fs::read_to_string(&script_path) {
        Err(_) => issues.push(format!("hook script not found: {}", script_path.display())),
        Ok(actual) => {
            if structural(&actual) != structural(&expected) {
                issues.push(format!(
                    "hook script drifted from the current template: {}",
                    script_path.display()
                ));
            }
        }
    }

    // 2. Settings entry points at the script.
    let entry_ok = settings_bash_commands(&settings_path)
        .iter()
        .any(|c| c == &entry_command);
    if !entry_ok {
        issues.push(format!(
            "settings.json Bash hook does not point at {entry_command}: {}",
            settings_path.display()
        ));
    }

    // 3. Foreign-hook scan (advisory).
    let scan = crate::hook_scan::scan(repo_root);
    let foreign: Vec<&crate::hook_scan::ScopedHook> = scan
        .iter()
        .filter(|h| h.kind == crate::hook_scan::HookKind::Foreign)
        .collect();

    if fix {
        let backup = if settings_path.exists() {
            Some(backup_file(&settings_path)?)
        } else {
            None
        };
        write_script(&script_path, &expected)?;
        crate::hook_takeover::set_sole_bash_hook(&settings_path, &entry_command, false)?;
        clear_sentinels();
        eprintln!("repoctx hook doctor: repaired.");
        eprintln!("  hook script : {}", script_path.display());
        eprintln!(
            "  settings    : {} → {entry_command}",
            settings_path.display()
        );
        if let Some(b) = backup {
            eprintln!("  backup      : {}", b.display());
        }
        if !foreign.is_empty() {
            eprintln!("  note: foreign hooks remain (see below) — they still race.");
            report_foreign(&foreign);
        }
        return Ok(());
    }

    if issues.is_empty() && foreign.is_empty() {
        eprintln!("repoctx hook doctor: healthy.");
        return Ok(());
    }
    eprintln!("repoctx hook doctor: issues found.");
    for i in &issues {
        eprintln!("  - {i}");
    }
    report_foreign(&foreign);
    if !issues.is_empty() {
        eprintln!(
            "Run `repoctx hook doctor{} --fix` to repair.",
            if global { " -g" } else { "" }
        );
    }
    std::process::exit(1);
}

fn report_foreign(foreign: &[&crate::hook_scan::ScopedHook]) {
    if foreign.is_empty() {
        return;
    }
    eprintln!("  foreign PreToolUse → Bash hooks (these race with repoctx):");
    for h in foreign {
        eprintln!("    [{}] {}", h.scope.label(), h.command);
    }
}

fn settings_bash_commands(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(arr) = root
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|p| p.as_array())
    {
        for entry in arr {
            if entry.get("matcher").and_then(|m| m.as_str()) != Some("Bash") {
                continue;
            }
            if let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) {
                for h in hooks {
                    if let Some(c) = h.get("command").and_then(|c| c.as_str()) {
                        out.push(c.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Clear the script's cached version-ok + rtk-missing sentinels so the
/// next hook run re-validates.
fn clear_sentinels() {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    if let Some(dir) = dir {
        let _ = std::fs::remove_file(dir.join("repoctx-hook-version-ok"));
        let _ = std::fs::remove_file(dir.join("repoctx-rtk-missing-warned"));
    }
}

fn prompt_yes_no(question: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    print!("{question} [{hint}] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(match line.trim().to_ascii_lowercase().as_str() {
        "" => default,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    })
}

fn write_script(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    make_executable(path);
    Ok(())
}

/// Best-effort `chmod +x` via subprocess — set so git records the
/// executable bit when the user commits the script. A no-op where `chmod`
/// is absent (e.g. Windows, where Git Bash users get the bit from git).
/// Spawning the tool keeps this compile-time platform-agnostic — no
/// OS-specific permission API. See 2026-06-11-platform-agnostic.md.
fn make_executable(path: &Path) {
    let _ = std::process::Command::new("chmod")
        .arg("+x")
        .arg(path)
        .status();
}

/// Ensure `.gitattributes` keeps the committed shell script LF + its
/// executable bit across platforms.
fn ensure_gitattributes(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".gitattributes");
    let line = "*.sh text eol=lf";
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(line);
    next.push('\n');
    std::fs::write(&path, next).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// v0.5.x migration signal: a `hook.chain_commands` row, split into rtk
/// presence + any other (non-auto-portable) commands. Read-only.
struct Migration {
    has_rtk: bool,
    others: Vec<String>,
}

fn detect_chain_commands(repo_root: &Path) -> Option<Migration> {
    // Only if a prior DB exists — don't create one just to check.
    if !repo_root.join(".repoctx/index.db").exists() {
        return None;
    }
    let store = Store::open(repo_root).ok()?;
    let raw = store.get_setting("hook.chain_commands").ok()??;
    let cmds: Vec<String> = raw
        .split('\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if cmds.is_empty() {
        return None;
    }
    let has_rtk = cmds.iter().any(|c| c.contains("rtk"));
    let others = cmds.into_iter().filter(|c| !c.contains("rtk")).collect();
    Some(Migration { has_rtk, others })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Copy `path` to a timestamped `.repoctx-backup-<unix-secs>` sibling.
fn backup_file(path: &Path) -> Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("settings.json");
    let backup = path.with_file_name(format!("{name}.repoctx-backup-{ts}"));
    std::fs::copy(path, &backup)
        .with_context(|| format!("back up {} → {}", path.display(), backup.display()))?;
    Ok(backup)
}
