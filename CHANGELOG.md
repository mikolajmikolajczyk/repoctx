# Changelog

All notable changes to this project will be documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/mikolajmikolajczyk/repoctx/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.2.0
[0.1.0]: https://github.com/mikolajmikolajczyk/repoctx/releases/tag/v0.1.0
