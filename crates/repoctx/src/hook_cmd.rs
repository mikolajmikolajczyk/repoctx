//! `repoctx hook` — per-agent install machinery.
//!
//! This file lands `hook list` and `hook status` only. `hook install` is
//! issue cd147ca.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use repoctx_integrations::{Fetcher, AGENTS};
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
    pub ref_: String,
    pub items: Vec<ListItem>,
}

impl HumanRender for ListReport {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("ref: {}\n", self.ref_));
        if self.items.is_empty() {
            out.push_str("(no agents)");
            return out;
        }
        let w = self.items.iter().map(|i| i.name.len()).max().unwrap_or(0);
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let desc = item.description.clone().unwrap_or_else(|| {
                "(description unavailable — try --ref main or --no-cache)".into()
            });
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
    pub ref_: String,
    pub dir: String,
    pub items: Vec<StatusAgent>,
}

impl HumanRender for StatusReport {
    fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("ref: {}\ndir: {}\n", self.ref_, self.dir));
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

pub fn run_list(fetcher: &Fetcher, render: Render) -> Result<()> {
    let items: Vec<ListItem> = AGENTS
        .iter()
        .map(|name| {
            let desc = fetcher.fetch_manifest(name).ok().map(|m| m.description);
            ListItem {
                name: (*name).into(),
                description: desc,
            }
        })
        .collect();
    let report = ListReport {
        count: items.len(),
        ref_: fetcher.ref_().to_string(),
        items,
    };
    crate::output::emit(&report, render)
}

pub fn run_status(fetcher: &Fetcher, dir: &Path, render: Render) -> Result<()> {
    let items: Vec<StatusAgent> = AGENTS
        .iter()
        .map(|name| match fetcher.fetch_manifest(name) {
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
        ref_: fetcher.ref_().to_string(),
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

/// Build the default Fetcher from CLI flags. Centralized so install
/// (issue cd147ca) reuses the exact same wiring.
pub fn build_fetcher(ref_: Option<String>, no_cache: bool) -> Result<Fetcher> {
    Fetcher::new(ref_, no_cache).context("fetcher init")
}

#[allow(dead_code)] // used by `hook status` default-dir resolution + install
pub fn resolve_dir(dir: Option<PathBuf>, repo_root: &Path) -> PathBuf {
    dir.unwrap_or_else(|| repo_root.to_path_buf())
}
