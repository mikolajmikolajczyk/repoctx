# ADR-0001 — CLI-first architecture

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: shape, ux, packaging

## Context

`repoctx` is a code intelligence CLI for AI agents (Claude Code, Codex, future IDE integrations). The primary caller is a coding agent that needs a fast, scriptable way to ask structural questions about a repository (`symbols`, `definition`, `outline`, `context`) without grepping. Interaction model: agent execs `repoctx <cmd>` per query, parses output. We need to pick a primary surface that the rest of the codebase is built around.

## Decision drivers

- Agents integrate easiest with stdin/stdout/JSON tools (no socket setup, no protocol negotiation).
- Scriptability and composability matter more than UI polish.
- Fast startup — agents may issue many short queries.
- A daemon can be layered on later (ADR-0005 introduces `repoctxd` for LSP); the reverse is harder.
- Reproducibility: a CLI is trivial to run in CI and in isolation.

## Considered options

1. **CLI-first**: single binary, invoked per query, persistence via SQLite on disk.
2. **Daemon-first**: long-running server with IPC; CLI is a thin client.
3. **Library-first**: ship a Rust crate; CLI is a sample consumer.

## Decision outcome

**CLI-first.** The binary is the product. MVP requires no daemon. Modules are organized so the eventual LSP-backed daemon (`repoctxd`, ADR-0005) is an additive component — the CLI keeps talking to a `CodeIntelBackend` trait (ADR-0004) and the daemon hides behind one of its impls.

## Positive consequences

- Trivial to integrate with agents and scripts (exec + parse JSON).
- No process-lifecycle, IPC, or auth surface in MVP.
- Stateless per-invocation queries against a persistent SQLite store keep failure modes simple.
- Fast cold path: open SQLite, query, exit.

## Negative consequences

- Per-invocation startup cost (process spawn + DB open). Acceptable for MVP; revisit if profiling shows it dominates.
- Interactive UX (streaming queries, watch mode) will eventually want a daemon — covered by `repoctxd` in ADR-0005.

## Links

- ADR-0003 (SQLite source of truth) — enables stateless CLI invocations.
- ADR-0004 (backend abstraction) — keeps CLI decoupled from backend choice.
- ADR-0005 (LSP via `repoctxd` daemon) — additive daemon, does not invert this decision.
- ADR-0008 (JSON output) — pairs with CLI-first to make output machine-consumable.
