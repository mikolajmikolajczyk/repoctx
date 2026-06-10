# ADR-0008 — TOON as default machine output, JSON opt-in

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: cli, ux, agents, output-format

## Context

The primary consumer of `repoctx` output is an AI coding agent — context windows and per-token cost dominate the experience. Standard JSON is verbose; identical data encoded in [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) uses ~30–40% fewer tokens on uniform arrays of objects, which is exactly what `symbols`, `outline`, `refs`, and most other `repoctx` outputs are. TOON is a lossless representation of the JSON data model, so tooling that wants real JSON loses nothing by asking for it.

## Decision drivers

- Agents are token-billed; default output should minimize tokens without losing structure.
- TOON's tabular layout fits `repoctx` outputs (rows of `Symbol`, `Location`, etc.) perfectly.
- Some consumers (existing scripts, dashboards, jq pipelines) want plain JSON. That has to stay first-class.
- Format choice must be a single CLI flag, not a global mode that surprises callers.
- TOON is a published spec (v3.3 at time of writing) with implementations across languages, including Rust — not a bespoke format we invent.

## Considered options

1. **TOON by default, `--json` opt-in.** Human-friendly text remains the default for an interactive TTY; piped/non-TTY output is TOON unless `--json` is passed.
2. **JSON default, `--toon` opt-in.** Familiar but burns tokens for the primary caller (agents).
3. **JSON only.** Simplest contract, leaves token savings on the table.
4. **YAML default.** Better than JSON on tokens, worse than TOON on uniform arrays; no real reason to pick a middle ground.

## Decision outcome

Every command whose output is meant to be consumed by another program emits **TOON by default** when output is not a TTY (or when `--toon` is explicitly passed). **`--json` switches to JSON.** Both shapes are documented and stable — they are two encodings of the same logical contract.

- Interactive (TTY) default: human-readable text.
- Non-interactive default: TOON.
- `--json`: JSON (canonical, RFC 8259).
- `--toon`: TOON (forces TOON even on a TTY).

The logical schema is owned by `backend` types (ADR-0004). The TOON and JSON encoders are two views over those types — adding a field is allowed (and arrives in both formats), renaming/removing is a breaking change announced as such.

## Positive consequences

- Default agent caller pays fewer tokens for the same answer.
- Existing JSON consumers keep working with a single `--json` flag.
- TOON is lossless: agents that prefer JSON internally can round-trip without information loss.
- The encoder is a layer over typed `backend` records, so the human formatter, TOON formatter, and JSON formatter all consume the same source of truth.

## Negative consequences

- Three output paths to keep coherent (human, TOON, JSON). Discipline required so shape drift doesn't appear between TOON and JSON.
- TOON is younger than JSON; tooling outside the LLM space is sparse. Users not pointing the output at an LLM should pass `--json`.
- One more thing to document per command.

## Links

- [TOON format](https://github.com/toon-format/toon) — Token-Oriented Object Notation, the encoding used as default machine output.
- [TOON spec](https://github.com/toon-format/spec) — normative reference.
- ADR-0001 (CLI-first) — output format is what makes CLI-first viable for agents.
- ADR-0004 (backend abstraction) — supplies the typed records that both encoders serialize.
