# Changelog

All notable changes to this project will be documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] — 2026-06-12

Per-repo config system + a rewritten README that finally explains the agent value story.

### Added

- **`repoctx config`** — new subcommand family (`show` / `get` / `set` / `unset`) over a persistent settings layer. Six initial keys: `hook.rewrite`, `hook.ref`, `hook.no_cache`, `gain.no_record`, `gain.record_query`, `output.default`. See [`wiki/user/config.md`](wiki/user/config.md).
- **Settings storage** — new `settings` key/value table inside `.repoctx/index.db`. Schema version bumps from `2` → `3`. The migration is idempotent + runs under the existing `BEGIN IMMEDIATE` guard alongside the v2 gain migration. Older binaries warn on unknown keys but don't crash, so newer-binary writes don't brick older readers.
- **Four-source precedence**: CLI flag → `REPOCTX_<SECTION>_<KEY>` env var → settings row → built-in default. `Source::{Cli, Env, Settings, Default}` tracked per field; `config show` annotates each row with its origin.
- **Persistent `output.default`** — `output::resolve` learned a third arg layering between `--json`/`--toon` flags and TTY detection. `config set output.default json` makes every read command emit JSON without `--json`. `auto` (default) preserves today's behavior (Human on TTY, TOON on pipe).
- **Persistent `gain.no_record` / `gain.record_query`** — same toggles as the existing CLI flags but reusable across invocations. CLI flag is "force on" — present flag wins over a `false` config, absent flag falls back to whatever the config layer resolved.
- **Persistent `hook.ref` / `hook.no_cache`** — defaults for the hook fetcher. Useful for teams that want every `hook install` invocation pinned to the same git ref.
- **`hook.rewrite` enum** (`auto`/`off`/`force`) plumbed end-to-end. The consumer — the transparent rewrite hook itself — lands in v0.5.0. Plumbing now means v0.4.0 users can pre-set the kill switch.
- **Legacy `RUST_REPOCTX_NO_RECORD` env var** still works as a back-compat alias for `REPOCTX_GAIN_NO_RECORD`. Documented as deprecated.

### Changed

- **README rewrite.** Drops the "Tree-sitter parses, SQLite stores" opener in favor of a concrete agent-pain narrative — `rg` returns 30 matches across 12 files, agent opens every file with `Read`, LLM pays the bill. Bench numbers from the helix codebase up front: 8,206 tokens (repoctx) vs 1,911,398 tokens (rg + every match). New "What it does" use-case bullets before any command syntax; new "How it works (short version)" demystifying the mental model without grammar / parser jargon.
- **`output::resolve` signature** — third parameter (`OutputDefault`) for the layered fallback. Tests cover all four precedence combinations.
- **`GainOpts::from_cli`** — takes the loaded `GainConfig` and OR-merges the CLI flag with it.
- **Decision doc** at `wiki/decisions/2026-06-12-config-schema.md` is the binding contract for the schema + precedence rules.

### Fixed

- The hard-coded gain `ENV_NO_RECORD` check in `gain::Recorder::record` is gone; the config layer's `apply_env` handles the env var resolution centrally. Removes a hidden behavior that bypassed the new precedence ladder.

### Notes

- Six config keys × four sources × one stored table = a small surface that grows additively. New keys are non-breaking; renamed/removed ones would be (we have none planned).
- Per-repo only. No global `~/.config/repoctx/`. Revisit only when a real cross-repo use case shows up.
- Hot reload is not implemented and won't be — each invocation reads fresh. Good enough for a CLI.

## [0.3.0] — 2026-06-12

Richer TypeScript / TSX coverage + a coverage-advisory layer so agents know when to fall back to `ripgrep`.

### Added

- **`repoctx languages`** — new subcommand surfacing the per-language coverage matrix. Returns `{slug, coverage: "full"|"partial", notes}` per supported language. Agents cache once per session to decide when to fall back to `ripgrep`.
- **Coverage advisory on every read command.** `outline`, `definition`, `context`, and `symbols` machine output now carries an optional top-level `advisory` field. Omitted in the happy path; populated when (a) the target file's language has `partial` coverage, (b) `--lang <slug>` filters to a `partial` language, or (c) `count == 0` and the workspace contains files in a `partial` language. Human render appends a final `advisory: <text>` line. The advisory always includes a concrete `rg -n <pattern>` fallback.
- **Richer TypeScript / TSX symbol coverage.** Vendored Aider's `typescript-tags.scm` (Apache-2.0) plus arrow-function patterns from Aider's `javascript-tags.scm` (same source). Plain `class`, plain `function`, arrow-function-assigned-to-`const`, `type` aliases, and `enum` declarations now surface across TS and TSX — they were all silently dropped by upstream `tree-sitter-typescript`. Empirical: a demo TSX file with React-style components went from 3/11 to 11/11 symbols captured.
- **`Language::coverage()` + `Language::notes()`** in `repoctx-index` — public surface so external callers can build their own routing logic on top of the same data the advisory layer uses.
- **`Language::from_slug()`** — inverse of `slug()`, used by the advisory layer to round-trip per-language counts from the store.
- **Coverage matrix in `wiki/user/commands.md`** with a `## repoctx languages` section and a separate "Coverage advisory on read commands" reference.

### Changed

- **TypeScript / TSX `kind` quirks table** in `wiki/user/commands.md` — drops the "plain `class`/`function` not surfaced" row (no longer true with the vendored query) and replaces it with the broader coverage explanation.
- **`integrations/shared/SKILL.md`** — new "Coverage advisory" section telling agents to check the field on every response and run the suggested `rg` command when present. The TS/TSX upstream-limitation paragraph is gone.
- **`wiki/agents/status.md`** — Languages line splits into "Full" and "Partial" blocks. Adds the `languages` subcommand + advisory mechanism to the Works section.
- **Extractor capture matcher** accepts both bare `@name` (upstream convention) and dotted `@name.definition.X` (Aider convention) so vendored queries plug in without rewriting captures. `definition_kind()` now maps `enum`, `struct`, `trait` to their typed kinds so Aider's enum captures land as `SymbolKind::Enum` instead of falling through to `other`.
- **`TreeSitterBackend::store()`** — new borrow method so command handlers can ask the store for per-language counts (needed by the advisory layer) without consuming the backend mid-pipeline.
- **`List<T>` output wrapper** — gains `advisory: Option<String>` field + `with_advisory()` builder. Skip-serialized in the happy path so machine output shape is unchanged for queries that don't trigger an advisory.

### Fixed

- **Enum capture mapping** — extractor now recognizes `@definition.enum` (previously fell through to `other`). The Aider-vendored queries depend on this; the same fix benefits any future grammar whose tags.scm uses the standard capture names.

### Notes

- The TS/TSX tags.scm vendor brings an Apache-2.0 dependency. `crates/index/queries/NOTICE` carries the attribution; LGPL-3.0-or-later compatible.
- Markdown stays at `full` coverage — heading-only is the right model for prose.
- JSON / YAML / TOML stay at `partial`. The opt-in nested-key extractor lives in issue `2c47040` (deferred to v0.6.0 — needs the config system from v0.4.0 to expose the switch cleanly).
- The `SymbolKind::Key` → `TopKey` rename (issue `1a19873`) was slated for v0.3.0 but deferred to a follow-up to keep the v0.3.0 surface focused on coverage transparency rather than a breaking-name change.

## [0.2.1] — 2026-06-11

Indexing now self-manages on every read; `--no-auto-index` removed; large docs sweep.

### Changed

- **Read commands auto-reindex before answering.** `symbols`, `outline`, `definition`, and `context` now run an incremental `index` pass before serving the answer — cheap on the no-op path (only files whose `(mtime_ns, size)` tuple changed get reparsed), quiet on stderr unless work happened. `status` and `gain` keep the lighter "build DB if missing, never reindex on top" behavior. Practical effect: `context`'s `stale: true` flag is effectively always `false` in normal use; the indexed `location` matches the on-disk source the agent reads.
- **`stale: true` documented honestly.** Previous wording suggested any read command would auto-fix staleness; in 0.2.0 only the missing-DB case triggered a reindex. The wording now matches the new behavior and points at `repoctx index` for edge cases.

### Removed

- **`--no-auto-index` global flag.** Indexing is what `repoctx` does; a flag to bypass it surfaces a "stale: true" edge case that wasn't useful in practice. Anyone who needs to assert "the index already exists, error otherwise" can `test -f .repoctx/index.db` in a script. This drops `Cli::no_auto_index` plus the `no_auto_index: bool` parameter on every read-command `run()` and removes four tests that asserted the bail path.

### Docs

- **README** — flagship `context` sample on top, three install paths (pre-built binaries with sha256 verification, `cargo install --git --tag`, `nix run` / `nix profile install`), quickstart spanning every read command, "Wire it into your coding agent" section advertising `hook install`, documentation index linking every wiki page.
- **`wiki/user/installation.md`** — rewritten. Pre-built binaries first with per-target table + curl + PowerShell snippets + sha256 step. Nix path simplified. Cargo path notes that `rusqlite` ships bundled SQLite. Verifying section names every shipped subcommand.
- **`wiki/agents/architecture.md`** — was labeled "Pre-alpha scaffold"; now reflects v0.2.x reality (current crate layout including `integrations`, command table covers every shipped command, data-flow includes hook install path and gain recording, distribution names the release workflow).
- **`wiki/agents/status.md`** — `Works` section includes hook + release CI; `Not started` now lists daemon/LSP + linux arm/musl + crates.io.
- **`wiki/agents/commands.md`** — bench + hook-e2e added to the test block; new Releasing section + Hook section for local dev with `REPOCTX_INTEGRATIONS_CACHE_DIR`.
- **`wiki/user/{commands,quickstart,gain,index,hook}.md`** — refreshed; `v0.1.0` example refs bumped to `v0.2.0`.
- **Removed milestone jargon** (`M0`/`M1`/`M1.5`/`M2`) from user-facing and agent-facing docs; the Radicle `milestone:*` labels are the source of truth. CHANGELOG and ADRs deliberately kept as-is.
- **`AGENTS.md`** — opener lists Codex and opencode alongside Claude Code; points at `wiki/user/hook.md`.

### Notes

- The `stale` field stays in the `context` machine output schema. It survives because the read-then-disk-read window isn't atomic — a file edited between the auto-reindex and the source read still flags. Effectively always `false` in normal use, but the field is the agent's safety net.
- Net change: `-149 lines` across the source tree; behavior simplification more than feature work.

## [0.2.0] — 2026-06-11

M1 navigation surface, M1.5 per-agent installer, gain wired everywhere, release-binary CI.

### Added

- **`repoctx outline <file>`** — document-symbol view of one file. Indented containment tree in human mode (stack-walk over symbols pre-ordered by `(start_line, start_column)`); flat `{count, items}` with 0-based ranges in machine mode. Path argument accepts repo-relative or absolute (canonicalized and re-anchored). File-not-in-index bails with a prescriptive error.
- **`repoctx definition <name>`** — exact-name (case-sensitive) lookup auto-filtered to the eight definition kinds (`function`, `method`, `class`, `interface`, `type`, `module`, `macro`, `constant`). Field/variable noise excluded so `definition path` doesn't drown in struct-field hits. `--lang`/`--limit` apply.
- **`repoctx context <symbol>`** — exact-name match plus a source window per hit. `--context C` lines either side (default 5), `--limit N` matches (default 3). Reads source from disk; sets `stale: true` when the file's current `(mtime_ns, size)` no longer matches the indexed tuple. Deleted-since-index files are dropped with a warn.
- **`repoctx hook list | status | install`** — per-agent install machinery for Claude Code, Codex, and opencode. Pulls manifests + files from the GitHub mirror at a pinned git ref (default `v<binary version>`), caches under XDG (override via `REPOCTX_INTEGRATIONS_CACHE_DIR`). Three modes — `write`, `append`, `merge-section`. Flags: `--dry-run`, `--force`, `--ref <git-ref>`, `--no-cache`. No `uninstall`: every install prints a per-file removal recipe.
- **Integrations content** in-tree under `integrations/`:
  - `shared/SKILL.md` — canonical Claude-skill-format guidance with frontmatter, command reference, prefer-this-over-that decision rules, kind/lang/limit semantics, upstream `tags.scm` quirks.
  - `shared/AGENTS.md.fragment` — codex + opencode AGENTS.md merge-section block.
  - `claude/CLAUDE.md.fragment` — Claude-specific CLAUDE.md guidance.
  - Per-agent manifests dispatching to the right destinations (`.claude/skills/repoctx/SKILL.md` for claude; `.agents/skills/repoctx/SKILL.md` for codex + opencode).
- **`repoctx-integrations`** workspace crate carrying the manifest schema, ureq+rustls HTTP fetcher with XDG cache layer, and installer with templated `{REPOCTX_BIN}` / `{REPO_NAME}` / `{REPO_ROOT}` variables.
- **Gain analytics now records every read command** including the M1 trio. `gain top` shows per-command savings breakdown across `symbols`, `outline`, `definition`, `context`.
- **Release-binary CI** — `.github/workflows/release.yml` builds four targets on every `v*` tag push (linux x86_64, macOS aarch64 + x86_64, windows x86_64), packages as tar.gz / zip with sha256 sidecars, attaches them to the matching GitHub release via `softprops/action-gh-release@v2`.
- **`wiki/user/hook.md`** — full reference for the install command: per-agent file table, mode reference, distribution model, template variables, removal recipe, troubleshooting.
- **`Store::file_exists` / `Store::file_stat`** — `outline` (file-in-index probe) and `context` (staleness check) ride these.

### Changed

- **`TreeSitterBackend::workspace_symbols`** — `limit = 0` now omits the LIMIT clause entirely instead of binding `usize::MAX`, which SQLite parsed past `i64::MAX` and returned "datatype mismatch". `definition` + `context` rely on the unlimited path.
- **`wiki/user/commands.md`** — three M1 sections under the M0 set; new `hook` section pointing at `hook.md`.
- **`wiki/user/quickstart.md`** — flagship `context` walkthrough now lives at step 4; gain moves to step 5.
- **`wiki/user/output-formats.md`** — CLAUDE.md recipe recommends all four read commands with "prefer this over X" guidance instead of just `symbols`.

### Fixed

- **macOS CI** — `directories` ignores `XDG_CACHE_HOME` on macOS, breaking the hook e2e suite. Added `REPOCTX_INTEGRATIONS_CACHE_DIR` env override (checked before `ProjectDirs`); tests + power users opt in via the env var, default path unchanged.
- **Windows CI** — `PathBuf::from("/etc/passwd").is_absolute()` returns false on Windows. `manifest::File::validate` now rejects `/`- or `\`-rooted dest strings explicitly so a manifest hostile on either platform fails fast.
- **Windows clippy** — `result_large_err` triggered on `IntegrationsError` because `PathBuf` is larger on Windows. Allowed at crate level with a one-line rationale; not a hot path.

### Notes

- Binary size grew ~2.5 MB (15→17.5 MB on x86_64-linux) — ureq + rustls + tokio-free TLS stack for the integrations fetcher.
- The integrations cache lives at `<XDG_CACHE_HOME>/repoctx/integrations/` on Linux, `~/Library/Caches/dev.repoctx.repoctx/integrations/` on macOS, `%LOCALAPPDATA%\repoctx\repoctx\cache\integrations\` on Windows, unless `REPOCTX_INTEGRATIONS_CACHE_DIR` is set.
- `opencode` integration assumes the `.agents/skills/<name>/SKILL.md` convention; verify against the live opencode version before relying on it.

## [0.1.0] — 2026-06-11

First tagged release. M0 functional surface complete on Linux, macOS, and Windows.

### Added

- **`repoctx index`** — incremental, mtime-based Tree-sitter indexing across 9 languages (Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown). Rayon parses in parallel, a single sequential SQLite writer persists, deleted files are pruned in one transaction. `--force` reparses everything.
- **`repoctx symbols <query>`** — case-insensitive substring search across the index with `--kind`, `--lang`, `--limit` filters and deterministic ordering. Empty result is exit 0 with `count: 0`.
- **`repoctx status`** — file/symbol counts, per-language breakdown, db size, schema version, and `(changed, new, deleted)` staleness from a stat walk. `--fast` skips the staleness walk.
- **`repoctx gain`** + **`repoctx gain top [--by saved|ratio]`** — surface the navigation tokens repoctx has avoided. Default window: last 30 days; `--since Nd|Nh|Nm|Ns` overrides; `--all` removes the window. `--history [N]` swaps the summary for the most recent rows.
- **Auto-index by default** — read commands (`symbols`, `status`, `gain`, `gain top`) auto-index when `.repoctx/index.db` is missing, printing one progress line to stderr. Scripts that want the old "bail with exit 1" can pass the new global `--no-auto-index` flag.
- **Output formats** — human (TTY default), [TOON](https://github.com/toon-format/toon) (non-TTY default), JSON (`--json`). `--json` and `--toon` are clap-mutually-exclusive. All three are encodings of the same typed records.
- **Backend abstraction** — `CodeIntelBackend` trait + `TreeSitterBackend` impl. Position-based methods (`definition`, `references`, `hover`) return a typed `Unsupported` until the M2 LSP backend lands.
- **Storage** — SQLite (`rusqlite` bundled). Schema v2: `files`, `symbols`, `meta`, `usage`. Migrations apply on open under `BEGIN IMMEDIATE` so two `repoctx index` processes on a fresh DB serialize cleanly. `busy_timeout` = 5 s; WAL on; `foreign_keys` ON.
- **Concurrency + corruption handling** — typed `Locked`/`Corrupted`/`NewerSchema` errors surface to the user with prescriptive messages and exit 1; never auto-delete the database.
- **Platform-agnostic** — no `std::os::unix` APIs; one path-helper pair (`to_db_path` / `from_db_path`) guards the fs boundary; CI on ubuntu/macos/windows + a `scripts/platform-check.sh` regression gate.
- **CI** — `.github/workflows/ci.yml` runs `fmt --check`, `build`, `test`, `clippy -D warnings`, and the platform check on every push/PR across all three desktop OSes.
- **Benchmarks** — `scripts/bench.sh` with hard budgets (cold 10 s / no-op 300 ms / symbols 100 ms / status 50 ms). Manual gate; baseline on a 5,000-file synthetic corpus: 318 / 50 / 3 / 5 ms.
- **Nix flake** — `packages.default` builds the release binary; `nix run github:mikolajmikolajczyk/repoctx` works.
- **User docs** under `wiki/user/`: install, quickstart, command reference, output formats + agent integration guide, gain analytics philosophy + privacy stance.

### Notes

- Distribution: source only. `nix run`, `nix profile install`, or `cargo install --git` are the supported paths. crates.io publishing is deferred until the API stabilizes.
- Release binary size: ~14 MB on x86_64-linux (9 statically-linked Tree-sitter grammars, accepted cost per ADR-0002).
- TypeScript upstream `tags.scm` covers interface / abstract class / method signatures only; plain `class`/`function` are not tagged. Documented in [`wiki/user/commands.md`](wiki/user/commands.md).

[Unreleased]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.4.0
[0.3.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.3.0
[0.2.1]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.2.1
[0.2.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.2.0
[0.1.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.1.0
