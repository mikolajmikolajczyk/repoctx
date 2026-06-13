//! `repoctx init` — first-class onboarding.
//!
//! Generates the committed `.repoctx/hook.sh` (dumb-pipe script), points
//! Claude Code's `PreToolUse → Bash` hook at it, writes `.gitattributes`,
//! and installs the agent guidance files. `-g` does the same at
//! user-global scope. See `wiki/decisions/2026-06-13-repoctx-init.md`.
//!
//! Out of scope here (separate issues): foreign-hook race detection
//! (`b2ad123`), `doctor` drift check (`2307c32`), v0.5.x migration
//! (`43142e1`), `--uninstall` (`ec698bb`).

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use repoctx_integrations::Installer;

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

    let rtk_present = crate::hook_rewrite::which("rtk").is_some();
    let mut rtk_chain = match opts.rtk {
        HookUseRtk::On => true,
        HookUseRtk::Off => false,
        HookUseRtk::Auto => rtk_present,
    };

    // Interactive confirmation only on a TTY without --yes.
    if !opts.yes && io::stdin().is_terminal() {
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

    // Scope-dependent paths.
    let (settings_path, script_path, entry_command) = if opts.global {
        let home = home_dir().context("cannot resolve home directory")?;
        let script = home.join(".claude/repoctx-hook.sh");
        let entry = script.display().to_string(); // absolute for global
        (home.join(".claude/settings.json"), script, entry)
    } else {
        (
            repo_root.join(".claude/settings.json"),
            repo_root.join(".repoctx/hook.sh"),
            ".repoctx/hook.sh".to_string(), // relative for project scope
        )
    };

    let version = env!("CARGO_PKG_VERSION");
    let script = crate::hook_script::render(rtk_chain, version, "repoctx");

    if opts.dry_run {
        eprintln!(
            "repoctx init (dry-run){}",
            if opts.global { " -g" } else { "" }
        );
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

    write_script(&script_path, &script)?;
    if !opts.global {
        ensure_gitattributes(repo_root)?;
    }
    crate::hook_takeover::set_sole_bash_hook(&settings_path, &entry_command, false)?;

    // Agent guidance files (project scope only — no project to write into
    // for a global install).
    if !opts.global {
        let repo_name = repo_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        Installer::new(repo_root.to_path_buf())
            .force(opts.force)
            .var("REPOCTX_BIN", "repoctx")
            .var("REPO_NAME", repo_name)
            .var("REPO_ROOT", repo_root.display().to_string())
            .install("claude")
            .context("install claude guidance files")?;
    }

    eprintln!("repoctx init: done.");
    eprintln!("  hook script : {}", script_path.display());
    eprintln!(
        "  settings    : {} → {entry_command}",
        settings_path.display()
    );
    eprintln!("  rtk chaining: {}", if rtk_chain { "on" } else { "off" });
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

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}
