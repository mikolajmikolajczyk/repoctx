//! `repoctx config` — read/write the per-repo settings table.

use std::path::Path;

use anyhow::{bail, Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::config::{self, Config};
use crate::output::{HumanRender, Render};

#[derive(Debug, Serialize)]
pub struct ConfigRow {
    pub key: String,
    pub value: String,
    pub default: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigReport {
    pub count: usize,
    pub items: Vec<ConfigRow>,
}

impl HumanRender for ConfigReport {
    fn human(&self) -> String {
        let w_key = self.items.iter().map(|i| i.key.len()).max().unwrap_or(0);
        let w_val = self.items.iter().map(|i| i.value.len()).max().unwrap_or(0);
        let w_src = self.items.iter().map(|i| i.source.len()).max().unwrap_or(0);
        let mut out = String::new();
        for (i, r) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!(
                "{key:<w_key$}  {value:<w_val$}  [{source:<w_src$}]  default={default}",
                key = r.key,
                value = r.value,
                source = r.source,
                default = r.default,
                w_key = w_key,
                w_val = w_val,
                w_src = w_src,
            ));
        }
        out
    }
}

pub fn run_show(repo_root: &Path, render: Render) -> Result<()> {
    let store = open_store(repo_root)?;
    let cfg = Config::load(&store).context("load config")?;
    let mut items = rows(&cfg);
    // hook.script_path is read-only + computed (where `init` wrote the
    // project hook script), not a stored setting.
    let script = repo_root.join(".repoctx/hook.sh");
    let (value, source) = if script.exists() {
        (".repoctx/hook.sh".to_string(), "computed")
    } else {
        ("(not installed)".to_string(), "default")
    };
    items.push(ConfigRow {
        key: "hook.script_path".to_string(),
        value,
        default: "(not installed)".to_string(),
        source: source.to_string(),
    });
    let report = ConfigReport {
        count: items.len(),
        items,
    };
    crate::output::emit(&report, render)
}

pub fn run_get(repo_root: &Path, key: String, render: Render) -> Result<()> {
    let store = open_store(repo_root)?;
    let cfg = Config::load(&store).context("load config")?;
    let row = rows(&cfg)
        .into_iter()
        .find(|r| r.key == key)
        .ok_or_else(|| anyhow::anyhow!("unknown config key: {key}"))?;
    crate::output::emit(
        &SingleRow {
            key: row.key.clone(),
            value: row.value,
            source: row.source,
        },
        render,
    )
}

pub fn run_set(repo_root: &Path, key: String, value: String) -> Result<()> {
    let mut store = open_store(repo_root)?;
    let normalized = config::set(&mut store, &key, &value)?;
    println!("set {key} = {normalized}");
    Ok(())
}

pub fn run_unset(repo_root: &Path, key: String) -> Result<()> {
    let mut store = open_store(repo_root)?;
    if !config::known_keys().iter().any(|(k, _)| *k == key) {
        bail!("unknown config key: {key}");
    }
    let deleted = store.delete_setting(&key)?;
    if deleted == 0 {
        println!("{key} was not set (default still applies)");
    } else {
        println!("unset {key} (default now applies)");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct SingleRow {
    key: String,
    value: String,
    source: String,
}

impl HumanRender for SingleRow {
    fn human(&self) -> String {
        format!("{}  {}  [{}]", self.key, self.value, self.source)
    }
}

fn open_store(repo_root: &Path) -> Result<Store> {
    Store::open(repo_root).context("open store")
}

fn rows(cfg: &Config) -> Vec<ConfigRow> {
    let defaults = config::known_keys();
    let default_for = |k: &str| -> String {
        defaults
            .iter()
            .find(|(dk, _)| *dk == k)
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    };
    vec![
        row(
            "hook.rewrite",
            cfg.hook.rewrite.as_str(),
            cfg.hook.rewrite_source.as_str(),
            &default_for("hook.rewrite"),
        ),
        row(
            "hook.use_rtk",
            cfg.hook.use_rtk.as_str(),
            cfg.hook.use_rtk_source.as_str(),
            &default_for("hook.use_rtk"),
        ),
        row(
            "hook.chainable",
            &cfg.hook.chainable.join(","),
            cfg.hook.chainable_source.as_str(),
            &default_for("hook.chainable"),
        ),
        row(
            "hook.chain_commands",
            &cfg.hook.chain_commands.join("\\n"),
            cfg.hook.chain_commands_source.as_str(),
            &default_for("hook.chain_commands"),
        ),
        row(
            "hook.telemetry",
            bool_str(cfg.hook.telemetry),
            cfg.hook.telemetry_source.as_str(),
            &default_for("hook.telemetry"),
        ),
        row(
            "hook.telemetry_samples",
            bool_str(cfg.hook.telemetry_samples),
            cfg.hook.telemetry_samples_source.as_str(),
            &default_for("hook.telemetry_samples"),
        ),
        row(
            "gain.no_record",
            bool_str(cfg.gain.no_record),
            cfg.gain.no_record_source.as_str(),
            &default_for("gain.no_record"),
        ),
        row(
            "gain.record_query",
            bool_str(cfg.gain.record_query),
            cfg.gain.record_query_source.as_str(),
            &default_for("gain.record_query"),
        ),
        row(
            "output.default",
            cfg.output.default.as_str(),
            cfg.output.default_source.as_str(),
            &default_for("output.default"),
        ),
        row(
            "index.nested_keys",
            bool_str(cfg.index.nested_keys),
            cfg.index.nested_keys_source.as_str(),
            &default_for("index.nested_keys"),
        ),
    ]
}

fn row(key: &str, value: &str, source: &str, default: &str) -> ConfigRow {
    ConfigRow {
        key: key.to_string(),
        value: value.to_string(),
        default: default.to_string(),
        source: source.to_string(),
    }
}

fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}
