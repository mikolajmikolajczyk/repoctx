# repoctx

AI-oriented repository intelligence CLI. **Tree-sitter** parses, **SQLite** stores, queries answer in milliseconds. Output defaults to [TOON](https://github.com/toon-format/toon) so the LLM on the other end of the pipe pays the fewest tokens for the same answer; `--json` for scripts.

```text
$ repoctx context resolve_window --context 2 --limit 1
# crates/repoctx/src/main.rs:241  resolve_window  function
  239  }
  240
  241  fn resolve_window(since: Option<&str>, all: bool) -> Result<gain_cmd::Window> {
  242      if all {
  243          return Ok(gain_cmd::Window::All);
  244      }
  ...
```

## Install

### Pre-built binaries (fastest)

Grab a release tarball from [releases](https://github.com/mikolajmikolajczyk/repoctx/releases). Four targets ship per tag:

| Target | Asset |
|---|---|
| Linux x86_64 | `repoctx-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `repoctx-<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `repoctx-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `repoctx-<version>-x86_64-pc-windows-msvc.zip` |

Each archive ships a `.sha256` sidecar — verify before unpacking:

```sh
shasum -a 256 -c repoctx-0.2.1-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf repoctx-0.2.1-x86_64-unknown-linux-gnu.tar.gz
sudo mv repoctx-0.2.1-x86_64-unknown-linux-gnu/repoctx /usr/local/bin/
```

### Cargo (from source)

```sh
cargo install --git https://github.com/mikolajmikolajczyk/repoctx --tag v0.2.1
```

### Nix

```sh
nix run github:mikolajmikolajczyk/repoctx
# or
nix profile install github:mikolajmikolajczyk/repoctx
```

## Quickstart

```sh
cd /path/to/your/repo
repoctx symbols UserService            # auto-indexes on first run
repoctx definition parse_config        # exact-name, definition kinds only
repoctx context resolve_window         # symbol + surrounding source
repoctx outline src/main.rs            # one file's symbol tree
repoctx status                         # counts + staleness
repoctx gain                           # how many tokens repoctx saved
```

All read commands auto-build the index on first run. Pass `--json` when piping into `jq`; default TOON is jq-incompatible by design.

## Wire it into your coding agent

`repoctx hook install <agent>` drops a skill + AGENTS.md guidance into the current repo so AI coding agents auto-load it:

```sh
repoctx hook install claude        # Claude Code
repoctx hook install codex         # OpenAI Codex
repoctx hook install opencode      # opencode
```

Same destination model as any agent skill — `.claude/skills/repoctx/SKILL.md` for Claude, `.agents/skills/repoctx/SKILL.md` for the rest, plus a merge-section block in `CLAUDE.md` or `AGENTS.md`. Idempotent re-installs, dry-run preview, removal recipe printed on success. Full reference: [`wiki/user/hook.md`](wiki/user/hook.md).

## What's in the box

- `index` — incremental walk + Tree-sitter parse + SQLite upsert; mtime-based invalidation.
- `symbols` — case-insensitive substring search; `--kind`, `--lang`, `--limit` filters.
- `outline` — document-symbol tree for one file (indented containment in human mode, flat in machine).
- `definition` — exact-name lookup, definition-kind whitelist (no struct-field noise).
- `context` — exact-name match + source window around each hit, with a `stale` flag against the indexed `(mtime, size)`.
- `status` — counts, per-language breakdown, freshness `{changed, new, deleted}`.
- `gain` — token-savings analytics ("navigation cost avoided").
- `hook` — per-agent install machinery.
- 9 languages: Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown.
- Three output formats over one set of typed records: human (TTY), TOON (pipes), JSON (`--json`).
- CI green on Linux + macOS + Windows.

Bench baseline on a 5,000-file synthetic corpus: cold index 318 ms, no-op incremental 50 ms, warm `symbols` query 3 ms.

## Documentation

User docs under [`wiki/user/`](wiki/user/index.md):

- [Installation](wiki/user/installation.md) — Nix, Cargo, binaries.
- [Quickstart](wiki/user/quickstart.md) — five-minute walk-through.
- [Commands reference](wiki/user/commands.md) — every flag, every exit code.
- [Hook — per-agent install](wiki/user/hook.md) — Claude Code / Codex / opencode integration.
- [Output formats + agent integration](wiki/user/output-formats.md) — TOON vs JSON vs human; CLAUDE.md recipe, jq snippets.
- [Gain analytics](wiki/user/gain.md) — what `gain` measures, baseline rules, privacy stance.

Agent docs (architecture, conventions, project status) under [`wiki/agents/`](wiki/agents/) — start at [`AGENTS.md`](AGENTS.md).

## Contributing

Canonical forge is **Radicle**. GitHub mirror exists for CI and discoverability only — patches and issues there aren't monitored.

```sh
rad clone rad:z3ZAf4PfKZnuurn2YNz3t7cTLLUgB
cd repoctx
nix develop                            # pinned toolchain + tooling
rad issue list --all
git push rad HEAD:refs/patches         # submit a patch
```

If Radicle isn't your thing, open a GitHub issue describing what you'd like to send and we'll figure it out.

## License

LGPL-3.0-or-later — see [`LICENSE`](LICENSE). Modifications to `repoctx` itself stay open; integration into proprietary workflows is fine.
