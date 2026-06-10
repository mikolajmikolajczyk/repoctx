# ADR-0004 — Code intelligence via backend abstraction

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: architecture, extensibility, api

## Context

Tree-sitter (ADR-0002) answers structural questions; LSP (ADR-0005, future) answers semantic ones. The CLI must not care which backend produced an answer — `symbols`, `definition`, `outline`, `context` should look the same to the caller whether Tree-sitter or `repoctxd`-proxied LSP served them. Future backends (SCIP indexes, language-specific tools) should slot in without rewriting commands.

## Decision drivers

- Callers (CLI commands) shouldn't know which backend answered.
- Backends differ in capability — Tree-sitter can do `symbols`/`outline`/`definition` (best-effort); LSP adds `references`/`hover`/etc.
- Adding a backend should not require touching CLI code.
- Trait should be small, query shapes should be reusable.

## Considered options

1. **Single `CodeIntelBackend` trait** with capability negotiation; multiple implementations.
2. **One enum** with hard-coded variants per backend.
3. **Bespoke wiring** per command, no abstraction.

## Decision outcome

A `CodeIntelBackend` trait lives in the `backend` crate. The CLI talks only to this trait. The default backend is `TreeSitterBackend`; ADR-0005 adds an `LspBackend` (proxied through `repoctxd`). Backends advertise capability so commands can degrade gracefully ("no LSP available → syntactic answer only" or a clean error).

### Trait shape (sketch)

```rust
pub trait CodeIntelBackend {
    fn workspace_symbols(&self, query: &SymbolQuery) -> Result<Vec<Symbol>>;
    fn document_symbols(&self, file: &Path)          -> Result<Vec<Symbol>>;
    fn definition(&self, query: &PositionQuery)      -> Result<Vec<Location>>;
    fn references(&self, query: &PositionQuery)      -> Result<Vec<Location>>;
    fn hover(&self, query: &PositionQuery)           -> Result<Option<HoverInfo>>;
}
```

Tree-sitter implements `workspace_symbols`, `document_symbols`, and a best-effort `definition`. `references` / `hover` return a typed `Unsupported` for the Tree-sitter backend and are only meaningful once `LspBackend` is wired in.

`SymbolQuery`, `PositionQuery`, `Symbol`, `Location`, `HoverInfo` are owned by the `backend` crate. The shapes are the public contract — adding fields is fine, renaming/removing is breaking (per ADR-0008's stability stance for `--json` output).

## Positive consequences

- Adding a backend doesn't ripple through CLI code.
- Capability negotiation lets the CLI report honest answers ("symbol's references require an LSP backend; none configured for this language").
- Testable in isolation with stub backends.
- The trait mirrors LSP request shapes closely — the LSP backend in `repoctxd` is mostly translation.

## Negative consequences

- One more layer of indirection.
- Capability negotiation is a small design surface that needs care to keep simple.

## Links

- ADR-0002 (Tree-sitter primary) — default `CodeIntelBackend` impl.
- ADR-0005 (LSP via `repoctxd`) — opt-in `CodeIntelBackend` impl that proxies to the daemon.
- ADR-0008 (JSON output) — stability constraints apply to the types returned by this trait when serialized.
