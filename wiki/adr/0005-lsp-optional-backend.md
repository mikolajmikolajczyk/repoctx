# ADR-0005 — LSP via an optional `repoctxd` daemon

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: backend, lsp, daemon, ipc

## Context

LSP gives richer semantics than Tree-sitter (`references`, `hover`, call hierarchy, type-aware `definition`, rename). But LSP servers are heavy: per-language install, slow startup, per-workspace warm-up, variable quality. The CLI (ADR-0001) is per-invocation and short-lived; embedding LSP clients in it would mean paying startup on every query and never benefiting from the server's incremental analysis.

## Decision drivers

- Default install must work without any language server present (ADR-0002).
- LSP-enhanced queries must be opt-in and per-language.
- LSP servers must stay warm across queries to be worth using at all — that needs a long-lived process.
- The CLI surface (ADR-0001) shouldn't change.
- The backend trait (ADR-0004) shouldn't change.

## Considered options

1. **Separate daemon `repoctxd`** that the CLI talks to over a unix socket; the daemon manages LSP child processes per workspace and translates between the `CodeIntelBackend` API and LSP JSON-RPC.
2. **Embed LSP clients in the CLI** behind a feature flag, spawn them per invocation (warm-up cost per query, defeats purpose).
3. **Embed LSP clients in the CLI** with a persistent on-disk handle to background processes (essentially reinventing a daemon, badly).
4. **No LSP** — Tree-sitter only, forever.

## Decision outcome

LSP support is delivered by a separate, optional daemon: **`repoctxd`**.

```
repoctx (CLI)
   │  unix socket (JSON, framed)
   ▼
repoctxd
   ├── workspace: project-a
   │     ├── gopls            (stdin/stdout, JSON-RPC)
   │     └── tsserver
   └── workspace: project-b
         └── rust-analyzer
```

### Responsibilities

- **`repoctx` (CLI)** — argument parsing, dispatch, output. Holds `TreeSitterBackend` directly. For LSP-eligible queries, holds an `LspBackend` impl whose body is "open the socket, send a request, parse the reply". Falls back gracefully if `repoctxd` isn't running or doesn't know the language.
- **`repoctxd`** — workspace registry, LSP child-process lifecycle (spawn, init, shutdown, restart on crash), JSON-RPC translation between our trait shapes (ADR-0004) and LSP messages, multiplexing concurrent CLI clients.
- **LSP server** — semantic analysis. `repoctxd` does not interpret semantics, only routes.

The CLI can run with no daemon at all — Tree-sitter answers what it can, semantic-only queries return a typed "no LSP backend available" error and the agent can act on it.

The CLI↔daemon transport is abstracted per platform: unix domain socket on unix, named pipe on Windows (the project is platform-agnostic — see `wiki/decisions/2026-06-11-platform-agnostic.md`). The framed-JSON protocol is transport-independent.

### Why not a feature flag in the CLI

Per-invocation LSP spawning forfeits the whole reason to use LSP. A long-lived process is required, and once we accept that, separating it from the CLI is cleaner than half-living state inside short-lived binaries.

## Positive consequences

- CLI install stays a single static binary with zero LSP dependencies.
- LSP servers stay warm across queries — the only way LSP-backed answers come back fast enough to be useful.
- Workspace registry lives in one place; `repoctxd` can hold several projects and several servers per project.
- Daemon failures (server crash, slow init) degrade gracefully: CLI falls back to Tree-sitter or returns a clean "unsupported" answer.
- Splitting the daemon means the CLI codebase stays free of LSP plumbing.

## Negative consequences

- Two binaries to ship and version (`repoctx`, `repoctxd`); protocol between them is now a contract.
- Lifecycle UX: when does `repoctxd` start? Foreground? `systemd --user`? Socket-activated? TBD; deferred until ADR-0005 is implemented.
- Crash recovery for stuck/crashing LSP children is real work — owned by `repoctxd`.

## Links

- ADR-0001 (CLI-first) — `repoctxd` is additive, not a reversal.
- ADR-0002 (Tree-sitter primary) — remains the default and always-available answer source.
- ADR-0004 (backend abstraction) — `LspBackend` is one more `CodeIntelBackend` impl; the daemon is an implementation detail behind it.
