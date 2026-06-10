# Coding conventions

Generic conventions that apply regardless of stack. Stack-specific rules (language idioms, framework patterns, formatter config) go in the **Stack-specific** section at the bottom — filled at bootstrap.

## File naming

- Pick **one** casing per category and stick with it (e.g. PascalCase for components, kebab-case for scripts, snake_case for modules). Document the choice in Stack-specific.
- One unit per file (one component, one class, one primary export). Co-locate tightly related sibling files (CSS module next to component, test next to source).

## Imports

- Cross-folder imports go through a folder's barrel / public entry, not into its internals. The barrel is the contract; internals are not.
- Prefer path aliases (`@core`, `@services`, ...) over deep relative paths once the project grows past ~3 directory levels.

## Comments

- **Default: no comment.** Names do the work.
- Add only when the *why* is non-obvious: hidden constraint, subtle invariant, workaround for a specific bug, surprising behavior.
- Never explain *what* the code does — well-named identifiers already do that.
- Don't reference the current task / fix / PR ("added for X", "handles case from #123") — that belongs in the commit message, not the source file.

## Commits

- Conventional Commits by default (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `release:`). If your project uses a different convention, document in Stack-specific.
- GPG-signed. The `gpg-uid-guard` pre-commit hook refuses to sign if `user.email` has no matching UID on `user.signingkey`.
- **Never commit without explicit user request.** This rule supersedes any plan acceptance.

## Phase / scope discipline

- Don't pre-empt later milestones. If something is tagged `milestone:m5`, don't half-implement it during M2 work.
- If a refactor would be cleaner alongside a bug fix but isn't required, defer it — open a Radicle issue instead.
- Don't add error handling, fallbacks, or validation for scenarios that can't happen at the call site. Trust internal code; validate only at system boundaries (user input, external APIs).

## UI / output text (if applicable)

- Terse. Lowercase. No emoji in UI text or logs unless the project explicitly opts in.

## When in doubt

- Read the relevant ADR in [`../adr/`](../adr/).
- Check Radicle issues for active work: `rad issue list --all`.
- Ask the user. Solo project — they're the only deciding authority.

---

## Stack-specific

### Rust

- **Edition**: 2021. MSRV tracks the stable toolchain pinned in `flake.nix`; bump deliberately, not opportunistically.
- **Module / file naming**: `snake_case` for modules and files. One primary type per file when the file is non-trivial; small related types may colocate.
- **Crate / workspace shape**: see [`architecture.md`](architecture.md). Cross-crate imports go through each crate's `lib.rs` public surface — never reach into another crate's internals.
- **Clippy**: `cargo clippy --all-targets -- -D warnings`. New lints earn either a fix or a justified `#[allow(...)]` with a one-line reason.
- **Formatting**: `cargo fmt` (default `rustfmt.toml` until we have a reason to deviate).
- **Error handling**: `anyhow` at the CLI boundary (`main.rs`, command handlers); `thiserror` for library/domain error types when stable variants matter. Never `unwrap()`/`expect()` outside `main`, `tests/`, or clearly-marked invariants.
- **Logging**: `tracing` with structured fields. No `println!` for diagnostics; reserve `println!` for human-facing CLI output and `serde_json` for `--json` output.
- **Machine output**: default is **TOON** ([toon-format/toon](https://github.com/toon-format/toon)) for non-TTY output; `--json` opts into JSON; `--toon` forces TOON on a TTY. Both shapes are stable, both derive from the same typed `backend` records. See [ADR-0008](../adr/0008-toon-default-machine-output.md).
- **Tests**: unit tests next to source (`#[cfg(test)] mod tests`); cross-crate / CLI tests under `tests/`. Prefer real SQLite (in-memory or tempdir) over mocks for storage code.

### Stack (M0)

Direct dependencies expected in M0:

- `clap` — CLI parsing
- `serde` + `serde_json` — output shapes, config (JSON encoder)
- TOON encoder — Rust impl per the [TOON spec](https://github.com/toon-format/spec); default machine output (ADR-0008)
- `anyhow` — top-level error handling at CLI boundary
- `rusqlite` — SQLite (ADR-0003)
- `tree-sitter` + per-language grammar crates — indexing (ADR-0002)
- `ignore` + `walkdir` — gitignore-aware tree walk
- `rayon` — parallel file parsing
- `tracing` + `tracing-subscriber` — structured logging

Deferred to post-M0:

- `tokio` — only needed once `repoctxd` arrives (ADR-0005)
- `notify` — filesystem watching, future
- `tower-lsp` (or equivalent) — LSP client inside `repoctxd`, future

### Commits

Conventional Commits with scope = crate or subsystem. Examples:

```
feat(index): add Tree-sitter symbol extraction
fix(sqlite): handle deleted files on incremental update
docs(adr): add backend-abstraction decision
```
