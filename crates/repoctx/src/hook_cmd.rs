//! `repoctx hook` — per-agent install machinery (list / status / install).

use std::path::{Path, PathBuf};

use anyhow::Result;
use repoctx_integrations::{content, InstallResult, Installer, AGENTS};
use serde::Serialize;

use crate::output::{HumanRender, Render};

#[derive(Debug, Serialize)]
pub struct ListItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListReport {
    pub count: usize,
    pub items: Vec<ListItem>,
}

impl HumanRender for ListReport {
    fn human(&self) -> String {
        let mut out = String::new();
        if self.items.is_empty() {
            out.push_str("(no agents)");
            return out;
        }
        let w = self.items.iter().map(|i| i.name.len()).max().unwrap_or(0);
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let desc = item.description.clone().unwrap_or_default();
            out.push_str(&format!(
                "{name:<w$}  {desc}",
                name = item.name,
                w = w,
                desc = desc
            ));
        }
        out
    }
}

#[derive(Debug, Serialize)]
pub struct StatusFile {
    pub dest: String,
    pub present: bool,
    pub mode: String,
}

#[derive(Debug, Serialize)]
pub struct StatusAgent {
    pub agent: String,
    pub files: Vec<StatusFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub count: usize,
    pub dir: String,
    pub items: Vec<StatusAgent>,
}

impl HumanRender for StatusReport {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("dir: {}\n", self.dir));
        for (i, a) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!("\n{}:\n", a.agent));
            if let Some(e) = &a.error {
                out.push_str(&format!("  (manifest unavailable: {e})"));
                continue;
            }
            for f in &a.files {
                let mark = if f.present { "✓" } else { "·" };
                out.push_str(&format!(
                    "  {mark}  {dest}  [{mode}]\n",
                    mark = mark,
                    dest = f.dest,
                    mode = f.mode
                ));
            }
            // strip trailing newline of this block
            if out.ends_with('\n') {
                out.pop();
            }
        }
        out
    }
}

pub fn run_list(render: Render) -> Result<()> {
    let items: Vec<ListItem> = AGENTS
        .iter()
        .map(|name| {
            let desc = content::manifest(name).ok().map(|m| m.description);
            ListItem {
                name: (*name).into(),
                description: desc,
            }
        })
        .collect();
    let report = ListReport {
        count: items.len(),
        items,
    };
    crate::output::emit(&report, render)
}

pub fn run_status(dir: &Path, render: Render) -> Result<()> {
    let items: Vec<StatusAgent> = AGENTS
        .iter()
        .map(|name| match content::manifest(name) {
            Ok(m) => {
                let files = m
                    .files
                    .into_iter()
                    .map(|f| StatusFile {
                        present: dir.join(&f.dest).exists(),
                        dest: f.dest,
                        mode: mode_str(f.mode).into(),
                    })
                    .collect();
                StatusAgent {
                    agent: (*name).into(),
                    files,
                    error: None,
                }
            }
            Err(e) => StatusAgent {
                agent: (*name).into(),
                files: Vec::new(),
                error: Some(format!("{e}")),
            },
        })
        .collect();

    let report = StatusReport {
        count: items.len(),
        dir: dir.display().to_string(),
        items,
    };
    crate::output::emit(&report, render)
}

fn mode_str(m: repoctx_integrations::Mode) -> &'static str {
    match m {
        repoctx_integrations::Mode::Write => "write",
        repoctx_integrations::Mode::Append => "append",
        repoctx_integrations::Mode::MergeSection => "merge-section",
    }
}

pub fn resolve_dir(dir: Option<PathBuf>, repo_root: &Path) -> PathBuf {
    dir.unwrap_or_else(|| repo_root.to_path_buf())
}

impl HumanRender for InstallResult {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "agent: {}{}\n",
            self.agent,
            if self.dry_run { " (dry-run)" } else { "" }
        ));
        out.push_str(&format!("dir:   {}\n\n", self.dir.display()));
        for w in &self.written {
            out.push_str(&format!(
                "  {:?}  {}  ({} bytes)\n",
                w.action,
                w.path.display(),
                w.bytes
            ));
        }
        out.push('\n');
        out.push_str(&self.removal);
        out
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_install(
    dir: &Path,
    repo_root: &Path,
    agent: &str,
    dry_run: bool,
    force: bool,
    repoctx_bin: &Path,
    render: Render,
) -> Result<()> {
    let repo_name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let install_result = Installer::new(dir.to_path_buf())
        .force(force)
        .dry_run(dry_run)
        .var("REPOCTX_BIN", repoctx_bin.display().to_string())
        .var("REPO_NAME", repo_name)
        .var("REPO_ROOT", dir.display().to_string())
        .install(agent)?;

    // For Claude, take ownership of `.claude/settings.json` so our
    // hook handler is the sole PreToolUse → Bash entry. Other agents
    // don't need this step — they don't have a hooks model we drive.
    let takeover = if agent == "claude" {
        Some(crate::hook_takeover::run(dir, dry_run)?)
    } else {
        None
    };

    // Persist displaced chain commands so `hook claude` chains them
    // on passthrough. Skipped during dry-run.
    if let Some(report) = &takeover {
        if !dry_run && !report.displaced_commands.is_empty() {
            if let Ok(mut store) = repoctx_store::Store::open(repo_root) {
                let serialized = report.displaced_commands.join("\n");
                if let Err(e) = crate::config::set(&mut store, "hook.chain_commands", &serialized) {
                    tracing::warn!(error = %e, "could not save hook.chain_commands");
                }
            }
        }
    }

    crate::output::emit(&install_result, render)?;

    if let Some(report) = takeover {
        let banner = if dry_run {
            "claude settings.json takeover (dry-run)"
        } else {
            "claude settings.json takeover"
        };
        eprintln!();
        eprintln!("{banner}: {}", report.settings_path.display());
        if report.displaced_commands.is_empty() {
            if report.already_owned {
                eprintln!("  already owned by repoctx — no changes");
            } else {
                eprintln!("  no prior Bash PreToolUse hooks; repoctx now sole owner");
            }
        } else {
            eprintln!(
                "  displaced {} prior Bash hook(s); chained via hook.chain_commands:",
                report.displaced_commands.len()
            );
            for cmd in &report.displaced_commands {
                eprintln!("    - {cmd}");
            }
            eprintln!("  to restore the prior setup, see the removal recipe above");
        }
        // Warn on user-global Bash hooks (project-scoped tool can't
        // fix them — Claude Code merges, doesn't override).
        if let Ok(scan) = crate::hook_takeover::scan_user_global() {
            crate::hook_takeover::warn_user_global(&scan);
        }
    }
    Ok(())
}
