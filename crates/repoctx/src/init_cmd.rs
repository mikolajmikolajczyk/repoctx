//! `repoctx init` — onboarding.
//!
//! Installs the agent guidance files (skill + `CLAUDE.md`/`AGENTS.md`) and, for
//! Claude, wires the `SessionStart` hook that injects `repoctx prime` at the
//! start of every session — the adoption-via-priming path
//! (decision 2026-06-16-adoption-via-priming). `-g` installs at user-global
//! scope (the global skill + `~/.claude/settings.json` SessionStart hook, which
//! then primes every repo). `--uninstall` reverses it.
//!
//! There is no longer a per-command PreToolUse rewrite hook: repoctx primes the
//! agent once instead of intercepting every command (the global `rtk` hook, if
//! present, keeps compressing grep/git on its own).

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use repoctx_integrations::Installer;

pub struct InitOpts {
    pub global: bool,
    pub agent: String,
    pub yes: bool,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run(repo_root: &Path, opts: InitOpts) -> Result<()> {
    let claude = opts.agent == "claude";
    let (settings_path, script_path, command) = claude_paths(repo_root, opts.global)?;

    if opts.dry_run {
        eprintln!(
            "repoctx init (dry-run){} — agent: {}",
            if opts.global { " -g" } else { "" },
            opts.agent
        );
        eprintln!("  would install: {} guidance files", opts.agent);
        if claude {
            eprintln!("  would write  : {} (managed `repoctx prime` block)", script_path.display());
            eprintln!(
                "  would wire   : SessionStart hook `{command}` in {}",
                settings_path.display()
            );
        }
        return Ok(());
    }

    if !opts.yes && io::stdin().is_terminal() {
        let scope = if opts.global { "user-global" } else { "this project" };
        if !prompt_yes_no(&format!("Install repoctx for {scope} ({})?", opts.agent), true)? {
            eprintln!("aborted.");
            return Ok(());
        }
    }

    // Agent guidance files. Project scope installs into the repo; global scope
    // installs the skill into the home dir.
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
        .install(&opts.agent)
        .with_context(|| format!("install {} guidance files", opts.agent))?;

    // SessionStart priming is Claude-specific (settings.json hooks). Other
    // agents pick repoctx up through their guidance files. The hook points at a
    // bashrc-style script whose managed block runs `repoctx prime`; users can
    // append their own session-start context below it.
    if claude {
        crate::session_hook::write_script(&script_path, false)?;
        crate::session_hook::install_settings(&settings_path, &command, false)?;
    }

    eprintln!("repoctx init: done.");
    eprintln!("  agent       : {}", opts.agent);
    eprintln!(
        "  skill       : {}",
        guidance_dir.join(".claude/skills/repoctx/SKILL.md").display()
    );
    if claude {
        eprintln!("  script      : {} (runs `repoctx prime`; extend it freely)", script_path.display());
        eprintln!("  SessionStart: {} → `{command}`", settings_path.display());
        eprintln!("  the agent is now primed at session start. Add your own");
        eprintln!("  context below the managed block in the script.");
    } else {
        eprintln!("  note: SessionStart priming is Claude-only; {} uses guidance files.", opts.agent);
    }
    Ok(())
}

/// `repoctx init --uninstall [-g]` — remove the SessionStart hook. Guidance
/// files are left in place (printed recipe), like the prior behavior.
pub fn run_uninstall(repo_root: &Path, global: bool, dry_run: bool) -> Result<()> {
    let (settings_path, script_path, command) = claude_paths(repo_root, global)?;

    if dry_run {
        eprintln!(
            "repoctx init --uninstall (dry-run){}",
            if global { " -g" } else { "" }
        );
        eprintln!(
            "  would remove SessionStart prime hook from {} + the managed block in {}",
            settings_path.display(),
            script_path.display()
        );
        return Ok(());
    }

    let removed = crate::session_hook::uninstall(&settings_path, &script_path, &command, false)?;
    eprintln!("repoctx init --uninstall: done.");
    if removed {
        eprintln!("  removed SessionStart prime hook: {}", settings_path.display());
        eprintln!("  stripped managed block from {} (kept any of your own lines)", script_path.display());
    } else {
        eprintln!("  no repoctx SessionStart hook found at {}", settings_path.display());
    }
    if !global {
        eprintln!("  left alone: .repoctx/index.db + config, CLAUDE.md, .claude/skills/repoctx/.");
        eprintln!("  to remove guidance: delete the repoctx block in CLAUDE.md + rm -r .claude/skills/repoctx");
        eprintln!("  to wipe the index + config: rm -rf .repoctx");
    } else {
        eprintln!("  left alone: ~/.claude/skills/repoctx/ (the global skill).");
        eprintln!("  to remove guidance: rm -rf ~/.claude/skills/repoctx");
    }
    Ok(())
}

/// Claude paths for a scope: `(settings file, session-start.sh, hook command)`.
///
/// Project scope writes the hook into **`settings.local.json`** — the personal,
/// auto-gitignored project settings — not the committed `settings.json`: a
/// SessionStart hook executes a command, so it's a per-developer opt-in, not
/// something to impose on (and prompt) the whole team. The inert guidance
/// (skill + CLAUDE.md) is what gets committed/shared. Global scope uses
/// `~/.claude/settings.json`, which is already the user's personal config.
/// Project uses a repo-relative command (Claude runs hooks with cwd = project
/// root); global uses the absolute script path.
fn claude_paths(repo_root: &Path, global: bool) -> Result<(PathBuf, PathBuf, String)> {
    if global {
        let home = home_dir().context("cannot resolve home directory")?;
        let script = home.join(".claude/hooks/session-start.sh");
        let command = format!("bash {}", script.display());
        Ok((home.join(".claude/settings.json"), script, command))
    } else {
        Ok((
            repo_root.join(".claude/settings.local.json"),
            repo_root.join(".claude/hooks/session-start.sh"),
            "bash .claude/hooks/session-start.sh".to_string(),
        ))
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

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}
