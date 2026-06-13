//! Gain analytics — Recorder that writes one usage row per read command.
//!
//! Privacy: only aggregates persist (epic 4dd57c8). Filenames cross this
//! module only to compute (candidate_files, candidate_bytes) via the store
//! helper, then are dropped. `query` is forwarded to the store only when
//! the caller passed `--record-query`.

use std::time::{SystemTime, UNIX_EPOCH};

use repoctx_store::{Store, UsageRecord};
use tracing::debug;

use crate::config::GainConfig;

#[derive(Debug, Clone, Copy, Default)]
pub struct GainOpts {
    pub no_record: bool,
    pub record_query: bool,
}

impl GainOpts {
    /// Build from CLI flags + the loaded `GainConfig`. CLI flags are
    /// "force on" — a present CLI flag wins over a `false` config; an
    /// absent flag falls back to the config value (which itself
    /// already accounts for env + settings table + default).
    pub fn from_cli(no_record: bool, record_query: bool, cfg: &GainConfig) -> Self {
        Self {
            no_record: no_record || cfg.no_record,
            record_query: record_query || cfg.record_query,
        }
    }
}

pub struct Recorder<'a> {
    store: &'a mut Store,
    opts: GainOpts,
}

impl<'a> Recorder<'a> {
    pub fn new(store: &'a mut Store, opts: GainOpts) -> Self {
        Self { store, opts }
    }

    /// Record one invocation. Fire-and-forget: any failure here degrades
    /// to a debug log so the user-visible command output is never
    /// disturbed by recording errors.
    pub fn record(
        &mut self,
        command: &str,
        query: Option<&str>,
        candidate_paths: &[String],
        rendered: &str,
        output_format: &str,
    ) {
        if self.opts.no_record {
            return;
        }
        let stored_query = if self.opts.record_query {
            query.map(str::to_string)
        } else {
            None
        };
        let (candidate_files, candidate_bytes) =
            match self.store.gain_baseline_for_files(candidate_paths) {
                Ok(p) => p,
                Err(e) => {
                    debug!(error = %e, "gain: baseline lookup failed; skipping record");
                    return;
                }
            };
        let estimated_baseline_tokens = candidate_bytes / 4;
        let returned_tokens = estimate_tokens(rendered) as i64;
        let ts_unix_ns = now_unix_ns();
        let rec = UsageRecord {
            ts_unix_ns,
            command: command.to_string(),
            candidate_files,
            candidate_bytes,
            estimated_baseline_tokens,
            returned_tokens,
            output_format: output_format.to_string(),
            query: stored_query,
        };
        if let Err(e) = self.store.record_usage(&rec) {
            debug!(error = %e, "gain: record_usage failed");
        }
    }
}

/// Shared read-command tail: render `value` to a buffer, write it to
/// stdout, and record one gain row from the rendered bytes. Used by
/// `symbols` / `outline` / `definition` / `context` so the emit+record
/// dance lives in one place.
pub fn emit_and_record<T>(
    value: &T,
    render: crate::output::Render,
    store: &mut Store,
    opts: GainOpts,
    command: &str,
    query: Option<&str>,
    candidate_paths: &[String],
) -> anyhow::Result<()>
where
    T: serde::Serialize + crate::output::HumanRender,
{
    let mut buf = Vec::new();
    crate::output::emit_to(&mut buf, value, render)?;
    std::io::Write::write_all(&mut std::io::stdout().lock(), &buf)?;
    let rendered = String::from_utf8_lossy(&buf).into_owned();
    Recorder::new(store, opts).record(command, query, candidate_paths, &rendered, render.name());
    Ok(())
}

/// Estimate token count from byte length at 4 bytes/token.
///
/// Deliberately the *same* heuristic the baseline side uses
/// (`candidate_bytes / 4`), so the savings ratio divides like-for-like.
/// Precise BPE counting (model-specific, e.g. cl100k) lives in the
/// bench suite's dedicated `tokens` helper, not in the shipped binary —
/// here a method-consistent estimate is more honest than a half-precise
/// ratio. See decision in issue 3a7fbc1.
pub fn estimate_tokens(s: &str) -> usize {
    s.len() / 4
}

fn now_unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_bytes_over_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("hello world"), 2); // 11 bytes / 4
    }
}
