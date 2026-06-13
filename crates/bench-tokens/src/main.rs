//! `tokens` — estimate a token count from stdin for the agent benchmark.
//!
//! Reads all of stdin and prints the estimate (one integer). Default
//! metric is `bytes / 4`, method-consistent with `repoctx gain` and the
//! bench design (`wiki/decisions/2026-06-13-agent-bench.md`) — so bench
//! thresholds don't depend on a model-specific tokenizer. Precise BPE
//! counting (cl100k) is intentionally not bundled here; add it behind a
//! flag if headline numbers ever need it.

use std::io::Read;

fn main() {
    let mut buf = Vec::new();
    if std::io::stdin().read_to_end(&mut buf).is_err() {
        eprintln!("tokens: failed to read stdin");
        std::process::exit(1);
    }
    println!("{}", buf.len() / 4);
}
