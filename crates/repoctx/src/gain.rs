//! Gain analytics — Recorder that writes one usage row per read command.
//!
//! Privacy: only aggregates persist (epic 4dd57c8). Filenames cross this
//! module only to compute (candidate_files, candidate_bytes) via the store
//! helper, then are dropped. `query` is forwarded to the store only when
//! the caller passed `--record-query`.

use std::time::{SystemTime, UNIX_EPOCH};

use repoctx_store::{Store, UsageRecord};
use tiktoken_rs::{cl100k_base_singleton, CoreBPE};
use tracing::debug;

const ENV_NO_RECORD: &str = "RUST_REPOCTX_NO_RECORD";

#[derive(Debug, Clone, Copy, Default)]
pub struct GainOpts {
    pub no_record: bool,
    pub record_query: bool,
}

impl GainOpts {
    pub fn from_cli(no_record: bool, record_query: bool) -> Self {
        Self {
            no_record,
            record_query,
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
        if self.opts.no_record || std::env::var_os(ENV_NO_RECORD).is_some() {
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
        let returned_tokens = count_tokens(rendered) as i64;
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

fn encoder() -> &'static CoreBPE {
    cl100k_base_singleton()
}

pub fn count_tokens(s: &str) -> usize {
    encoder().encode_ordinary(s).len()
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
    fn count_tokens_nonempty() {
        assert!(count_tokens("hello world") > 0);
        assert_eq!(count_tokens(""), 0);
    }
}
