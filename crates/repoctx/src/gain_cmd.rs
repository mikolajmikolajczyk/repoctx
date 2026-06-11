//! `repoctx gain` + `repoctx gain top` — surface navigation cost avoided.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use repoctx_store::{CommandBreakdown, GainTotals, Store, UsageRecord};
use serde::Serialize;

use crate::output::{HumanRender, Render};
use crate::read_cmd;

const DEFAULT_WINDOW_DAYS: u64 = 30;
const NS_PER_SEC: i64 = 1_000_000_000;

#[derive(Debug, Clone, Copy)]
pub enum TopBy {
    Saved,
    Ratio,
}

impl TopBy {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "saved" => Ok(Self::Saved),
            "ratio" => Ok(Self::Ratio),
            other => bail!("--by must be 'saved' or 'ratio' (got '{other}')"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GainSummary {
    pub window: String,
    pub commands: u64,
    pub returned_tokens: i64,
    pub estimated_baseline_tokens: i64,
    pub estimated_savings: i64,
    pub reduction: f64, // percent, one decimal
}

impl HumanRender for GainSummary {
    fn human(&self) -> String {
        if self.commands == 0 {
            return format!("{}\nno recorded invocations in window", self.window);
        }
        format!(
            "{w}\n\nCommands:\n  {c}\n\nReturned:\n  {ret} tokens\n\nEstimated baseline:\n  {base} tokens\n\nReduction:\n  {red}\n\nEstimated savings:\n  {sav}",
            w = self.window,
            c = thousands(self.commands as i64),
            ret = thousands(self.returned_tokens),
            base = thousands(self.estimated_baseline_tokens),
            red = format_pct(self.reduction),
            sav = abbrev_tokens(self.estimated_savings),
        )
    }
}

#[derive(Debug, Serialize)]
pub struct CommandRow {
    pub command: String,
    pub commands: u64,
    pub returned_tokens: i64,
    pub estimated_baseline_tokens: i64,
    pub estimated_savings: i64,
    pub reduction: f64,
}

#[derive(Debug, Serialize)]
pub struct TopList {
    pub window: String,
    pub by: &'static str,
    pub count: usize,
    pub items: Vec<CommandRow>,
}

impl HumanRender for TopList {
    fn human(&self) -> String {
        let mut out = format!("{}\nby: {}\n", self.window, self.by);
        if self.items.is_empty() {
            out.push_str("\nno recorded invocations in window");
            return out;
        }
        for r in &self.items {
            out.push_str(&format!(
                "\n{cmd}:\n  {red} reduction · {sav} saved · {n} call(s)\n",
                cmd = r.command,
                red = format_pct(r.reduction),
                sav = abbrev_tokens(r.estimated_savings),
                n = thousands(r.commands as i64),
            ));
        }
        out
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryRow {
    pub ts_iso: String,
    pub command: String,
    pub candidate_files: u32,
    pub candidate_bytes: i64,
    pub estimated_baseline_tokens: i64,
    pub returned_tokens: i64,
    pub output_format: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryList {
    pub window: String,
    pub count: usize,
    pub items: Vec<HistoryRow>,
}

impl HumanRender for HistoryList {
    fn human(&self) -> String {
        let mut out = format!("{}\n", self.window);
        if self.items.is_empty() {
            out.push_str("no recorded invocations in window");
            return out;
        }
        for r in &self.items {
            out.push_str(&format!(
                "\n{ts}  {cmd:<10}  returned={ret}  baseline={base}  files={cf}  format={fmt}",
                ts = r.ts_iso,
                cmd = r.command,
                ret = thousands(r.returned_tokens),
                base = thousands(r.estimated_baseline_tokens),
                cf = r.candidate_files,
                fmt = r.output_format,
            ));
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Window {
    Since(i64),
    All,
}

impl Window {
    pub fn since_ns(self) -> Option<i64> {
        match self {
            Self::Since(ns) => Some(ns),
            Self::All => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::All => "All time".into(),
            Self::Since(ns) => {
                let now = now_unix_ns();
                let secs = (now - ns) / NS_PER_SEC;
                let days = secs / 86_400;
                if days >= 1 {
                    format!("Last {days} day{}", if days == 1 { "" } else { "s" })
                } else {
                    let hours = secs / 3_600;
                    if hours >= 1 {
                        format!("Last {hours} hour{}", if hours == 1 { "" } else { "s" })
                    } else {
                        let mins = secs / 60;
                        format!("Last {mins} minute{}", if mins == 1 { "" } else { "s" })
                    }
                }
            }
        }
    }
}

/// Parse `7d` / `2h` / `30m` / `120s`. Returns the absolute unix-ns lower bound.
pub fn parse_since(spec: &str) -> Result<i64> {
    let secs = parse_duration_secs(spec)?;
    let now = now_unix_ns();
    Ok(now - secs as i64 * NS_PER_SEC)
}

fn parse_duration_secs(spec: &str) -> Result<u64> {
    let spec = spec.trim();
    if spec.is_empty() {
        bail!("--since requires a duration like 7d / 2h / 30m");
    }
    let (num, unit) = spec.split_at(spec.len() - 1);
    let n: u64 = num
        .parse()
        .with_context(|| format!("invalid --since: '{spec}'"))?;
    Ok(match unit {
        "d" => n * 86_400,
        "h" => n * 3_600,
        "m" => n * 60,
        "s" => n,
        other => bail!("--since unit must be d/h/m/s (got '{other}')"),
    })
}

pub fn default_window() -> Window {
    let now = now_unix_ns();
    Window::Since(now - DEFAULT_WINDOW_DAYS as i64 * 86_400 * NS_PER_SEC)
}

pub fn run_summary(
    repo_root: &Path,
    window: Window,
    render: Render,
    history: Option<usize>,
) -> Result<()> {
    read_cmd::ensure_db(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let totals = store.gain_totals(window.since_ns())?;
    let label = window.label();
    let summary = summarize(&totals, label.clone());

    if let Some(limit) = history {
        let rows = store.gain_recent(limit)?;
        let cutoff = window.since_ns().unwrap_or(i64::MIN);
        let items: Vec<HistoryRow> = rows
            .into_iter()
            .filter(|r| r.ts_unix_ns >= cutoff)
            .map(to_history_row)
            .collect();
        let list = HistoryList {
            window: label,
            count: items.len(),
            items,
        };
        return crate::output::emit(&list, render);
    }
    crate::output::emit(&summary, render)
}

pub fn run_top(repo_root: &Path, window: Window, by: TopBy, render: Render) -> Result<()> {
    read_cmd::ensure_db(repo_root)?;
    let store = Store::open(repo_root).context("open store")?;
    let rows = store.gain_per_command(window.since_ns())?;
    let mut items: Vec<CommandRow> = rows.into_iter().map(to_command_row).collect();
    match by {
        TopBy::Saved => items.sort_by(|a, b| {
            b.estimated_savings
                .cmp(&a.estimated_savings)
                .then_with(|| a.command.cmp(&b.command))
        }),
        TopBy::Ratio => items.sort_by(|a, b| {
            b.reduction
                .partial_cmp(&a.reduction)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.command.cmp(&b.command))
        }),
    }
    let list = TopList {
        window: window.label(),
        by: match by {
            TopBy::Saved => "saved",
            TopBy::Ratio => "ratio",
        },
        count: items.len(),
        items,
    };
    crate::output::emit(&list, render)
}

fn summarize(t: &GainTotals, window: String) -> GainSummary {
    let savings = t.estimated_baseline_tokens - t.returned_tokens;
    let reduction = if t.estimated_baseline_tokens > 0 {
        (savings as f64) * 100.0 / (t.estimated_baseline_tokens as f64)
    } else {
        0.0
    };
    GainSummary {
        window,
        commands: t.invocations,
        returned_tokens: t.returned_tokens,
        estimated_baseline_tokens: t.estimated_baseline_tokens,
        estimated_savings: savings,
        reduction: round1(reduction),
    }
}

fn to_command_row(b: CommandBreakdown) -> CommandRow {
    let savings = b.estimated_baseline_tokens - b.returned_tokens;
    let reduction = if b.estimated_baseline_tokens > 0 {
        (savings as f64) * 100.0 / (b.estimated_baseline_tokens as f64)
    } else {
        0.0
    };
    CommandRow {
        command: b.command,
        commands: b.invocations,
        returned_tokens: b.returned_tokens,
        estimated_baseline_tokens: b.estimated_baseline_tokens,
        estimated_savings: savings,
        reduction: round1(reduction),
    }
}

fn to_history_row(r: UsageRecord) -> HistoryRow {
    HistoryRow {
        ts_iso: format_ts_iso(r.ts_unix_ns),
        command: r.command,
        candidate_files: r.candidate_files,
        candidate_bytes: r.candidate_bytes,
        estimated_baseline_tokens: r.estimated_baseline_tokens,
        returned_tokens: r.returned_tokens,
        output_format: r.output_format,
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn now_unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// ISO-8601 UTC timestamp like `2026-06-11T14:23:05Z` (no chrono dep).
fn format_ts_iso(ns: i64) -> String {
    let secs = ns / NS_PER_SEC;
    let (year, month, day, hour, minute, second) = secs_to_ymdhms(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Civil-from-days, Howard Hinnant 2010 ("date.h"). Public domain algorithm.
fn secs_to_ymdhms(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let hour = (tod / 3_600) as u32;
    let minute = ((tod % 3_600) / 60) as u32;
    let second = (tod % 60) as u32;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (if m <= 2 { y + 1 } else { y }) as i32;
    (y, m, d, hour, minute, second)
}

fn thousands(mut n: i64) -> String {
    if n == 0 {
        return "0".into();
    }
    let neg = n < 0;
    if neg {
        n = -n;
    }
    let mut digits: Vec<char> = n.to_string().chars().collect();
    let mut out = String::new();
    for (i, c) in digits.drain(..).rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        out.push('-');
    }
    out.chars().rev().collect()
}

fn abbrev_tokens(n: i64) -> String {
    let a = n.unsigned_abs() as f64;
    let sign = if n < 0 { "-" } else { "" };
    let (val, unit) = if a >= 1_000_000.0 {
        (a / 1_000_000.0, "M")
    } else if a >= 1_000.0 {
        (a / 1_000.0, "K")
    } else {
        return format!("{sign}{n} tokens");
    };
    format!("{sign}{val:.1}{unit} tokens")
}

fn format_pct(pct: f64) -> String {
    format!("{pct:.1}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separator() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(92_114), "92,114");
        assert_eq!(thousands(4_201_883), "4,201,883");
    }

    #[test]
    fn abbreviation() {
        assert_eq!(abbrev_tokens(500), "500 tokens");
        assert_eq!(abbrev_tokens(12_345), "12.3K tokens");
        assert_eq!(abbrev_tokens(4_123_456), "4.1M tokens");
    }

    #[test]
    fn pct_rounding() {
        assert_eq!(format_pct(97.83), "97.8%");
        assert_eq!(format_pct(0.0), "0.0%");
    }

    #[test]
    fn parse_since_units() {
        // Smoke: parse returns a value < now_unix_ns(). Exact magnitude omitted
        // (no Date::now stubbing); the unit handling is exercised here.
        let now = now_unix_ns();
        assert!(parse_since("7d").unwrap() < now);
        assert!(parse_since("2h").unwrap() < now);
        assert!(parse_since("30m").unwrap() < now);
        assert!(parse_since("120s").unwrap() < now);
    }

    #[test]
    fn parse_since_rejects_bad_input() {
        assert!(parse_since("").is_err());
        assert!(parse_since("7x").is_err());
        assert!(parse_since("abcd").is_err());
    }

    #[test]
    fn format_ts_iso_smoke() {
        // 2025-01-01T00:00:00Z = 1735689600 unix seconds
        let ns = 1_735_689_600_i64 * NS_PER_SEC;
        assert_eq!(format_ts_iso(ns), "2025-01-01T00:00:00Z");
    }
}
