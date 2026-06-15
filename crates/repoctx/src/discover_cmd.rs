//! `repoctx discover` — hook passthrough telemetry report (issue #7).
//!
//! Aggregates the `hook_events` table the PreToolUse hook records: per grep/
//! rg/find idiom, how often it was rewritten to repoctx vs leaked to grep
//! (passthrough) vs chained to rtk. Ranks by volume so the biggest adoption
//! gaps surface first — the data that drives which idioms to rewrite next.

use std::path::Path;

use anyhow::{Context, Result};
use repoctx_store::Store;
use serde::Serialize;

use crate::output::{emit, HumanRender, Render};

/// Per-idiom rollup across all tools and outcomes.
#[derive(Debug, Clone, Serialize)]
pub struct IdiomRow {
    pub idiom: String,
    pub rewritten: u64,
    pub passthrough: u64,
    pub chained: u64,
    pub total: u64,
    /// Share rewritten to repoctx, 0–100. The adoption signal.
    pub rewritten_pct: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverReport {
    pub events: u64,
    pub idioms: Vec<IdiomRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl HumanRender for DiscoverReport {
    fn human(&self) -> String {
        if self.idioms.is_empty() {
            return "no hook telemetry recorded yet — run some grep/rg/find \
                    commands in a repoctx-indexed repo first (or hook.telemetry is off)"
                .to_string();
        }
        let mut s = format!("hook passthrough telemetry ({} events)\n", self.events);
        s.push_str(&format!(
            "{:<18}  {:>9}  {:>11}  {:>7}  {:>6}  {}\n",
            "idiom", "rewritten", "passthrough", "chained", "total", "rewritten%"
        ));
        for r in &self.idioms {
            s.push_str(&format!(
                "{:<18}  {:>9}  {:>11}  {:>7}  {:>6}  {:>3}%\n",
                r.idiom, r.rewritten, r.passthrough, r.chained, r.total, r.rewritten_pct
            ));
        }
        if let Some(a) = &self.advisory {
            s.push_str("\nadvisory: ");
            s.push_str(a);
        }
        s.trim_end().to_string()
    }
}

pub fn run(repo_root: &Path, render: Render) -> Result<()> {
    // Read-only: report what exists. Don't create a DB just to say "empty".
    if !repo_root.join(".repoctx/index.db").exists() {
        let report = DiscoverReport {
            events: 0,
            idioms: Vec::new(),
            advisory: Some(
                "no index DB here — repoctx isn't active in this repo, so no hook \
                 telemetry was recorded"
                    .to_string(),
            ),
        };
        return emit(&report, render);
    }

    let store = Store::open(repo_root).context("open store")?;
    let stats = store.hook_event_stats(None)?;

    use std::collections::BTreeMap;
    let mut by_idiom: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let mut events = 0u64;
    for s in &stats {
        events += s.count;
        let e = by_idiom.entry(s.idiom.clone()).or_default();
        match s.outcome.as_str() {
            "rewritten" => e.0 += s.count,
            "passthrough" => e.1 += s.count,
            "chained" => e.2 += s.count,
            _ => {}
        }
    }

    let mut idioms: Vec<IdiomRow> = by_idiom
        .into_iter()
        .map(|(idiom, (rewritten, passthrough, chained))| {
            let total = rewritten + passthrough + chained;
            let rewritten_pct = (rewritten * 100).checked_div(total).unwrap_or(0) as u32;
            IdiomRow {
                idiom,
                rewritten,
                passthrough,
                chained,
                total,
                rewritten_pct,
            }
        })
        .collect();
    // Biggest buckets first = where the adoption gap matters most.
    idioms.sort_by(|a, b| b.total.cmp(&a.total).then(a.idiom.cmp(&b.idiom)));

    let advisory = discover_advisory(&idioms);
    let report = DiscoverReport {
        events,
        idioms,
        advisory,
    };
    emit(&report, render)
}

/// Point at the biggest leak: the highest-volume idiom that's mostly
/// passing through rather than being rewritten.
fn discover_advisory(idioms: &[IdiomRow]) -> Option<String> {
    let leak = idioms
        .iter()
        .filter(|r| r.total >= 5 && r.rewritten_pct < 50)
        .max_by_key(|r| r.passthrough)?;
    Some(format!(
        "biggest adoption gap: `{}` ({} passthrough / {} total, {}% rewritten) — \
         a candidate for a new hook rewrite rule",
        leak.idiom, leak.passthrough, leak.total, leak.rewritten_pct
    ))
}
