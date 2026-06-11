# Gain analytics: tokenizer = tiktoken cl100k_base; baseline = candidate_bytes / 4

**Date:** 2026-06-11
**Decider:** Mikołaj Mikołajczyk
**Tags:** library-choice, gain

## Context

Epic 4dd57c8 introduces `repoctx gain` — surfacing how many tokens an AI agent did NOT have to read because repoctx provided a narrower answer than grep + open-files. The number is meaningful only if both sides of the ratio are tokenized the same way, and the baseline must be computable from the index alone (no extra file IO).

## Decision

- **Tokenizer**: [`tiktoken-rs`](https://crates.io/crates/tiktoken-rs) with `cl100k_base`. Exact for current OpenAI models, approximate for Claude. Used on both the returned bytes and the baseline approximation, so the ratio is honest.
- **Baseline approximation**: `estimated_baseline_tokens = candidate_bytes / 4`. Re-reading every candidate file just to tokenize it would cost the very IO repoctx is meant to avoid — defeating the gain. Storing the bytes-only count + a fixed divisor keeps recording cheap and is good enough for ratio reporting.
- **`candidate_bytes` source**: summed from `files.size` in the store (recorded during indexing). Only the resulting count + bytes per invocation persist in `usage`; the filename list itself is touched transiently at record time and dropped.

## Alternatives considered

- **Anthropic's tokenizer** — no crate published with the model vocab; falling back to cl100k keeps both sides under one tokenizer.
- **Read + tokenize each candidate file** — exact, but eats the savings. Rejected.
- **Token count from `wc -w * 1.3`** — even rougher than `bytes / 4`, no advantage.

## Trigger to revisit

- A Claude-vocab tokenizer ships as a Rust crate.
- Real agent transcripts show the bytes-per-token ratio drifting far from 4 for representative code corpora, making the gain number misleading.
