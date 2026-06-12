# repoctx

**A CLI that makes your AI coding agent stop reading whole files.**

When Claude / Codex / opencode answers "where is `parse_config` defined?",
it usually does this:

1. Run `rg parse_config` — gets 30 line-matches across 12 files.
2. Open every candidate file with `Read` to figure out which one is
   the real definition.
3. Pay the LLM token bill for **every byte of every file it opened**.

`repoctx` replaces that whole loop with one structural query:

```sh
$ repoctx definition parse_config
crates/repoctx/src/config.rs:42  parse_config  function
```

One file path, one line, one symbol kind. The agent doesn't open
anything else — it already has the answer.

**On a real run** against the [helix](https://github.com/helix-editor/helix)
codebase (~150k LOC Rust), across 12 representative agent queries:

| Path | Tokens spent |
|---|---|
| repoctx | **8,206** |
| `ripgrep` + open top-match file | 370,896 (45× more) |
| `ripgrep` + open every matched file | 1,911,398 (233× more) |

That's a **97-99% reduction** in token cost — same answers, far less
input the LLM has to chew on.

## What it does

`repoctx` indexes your repo once into a tiny SQLite database (under
`.repoctx/index.db`), then answers structural questions about your
code in milliseconds:

- "Where is `X` defined?"
- "Show me `X` with the surrounding 5 lines."
- "What's the symbol structure of this file?"
- "Which functions / classes / types match this pattern?"

It's **not** a search tool — `rg` already does that better. It's a
**structural** tool. Each answer comes with the exact file, line,
column, and symbol kind. The agent receives a tiny structured
response instead of a wall of source code to grep through.

### Example queries

```sh
repoctx symbols UserService            # find symbols matching a substring
repoctx definition parse_config        # exact-name lookup ("where is X")
repoctx context resolve_window         # "show me X with code around it"
repoctx outline src/main.rs            # structure of one file
repoctx status                         # how big is the index, is it stale
repoctx gain                           # how many tokens have I saved so far
```

A complete `context` response looks like this:

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

Compare that to `rg "fn resolve_window"` (line-matches only, no
context, no symbol info) or `rg resolve_window | head` + `cat src/main.rs`
(whole file). The agent gets a focused answer in 100 tokens instead
of 4,000.

## How it works (the short version)

- A one-time index walks your repo and extracts named symbols
  (functions, classes, methods, etc.) plus their location. Stored in
  a local SQLite file at `.repoctx/index.db`.
- Read commands re-check changed files first (cheap — only files whose
  `(mtime, size)` differ get reparsed), then answer from the index.
- Default output is [TOON](https://github.com/toon-format/toon), a
  format LLM tokenizers compress about 5× better than JSON. Pass
  `--json` if you're piping into `jq`.
- 9 languages supported with full coverage: Go, Rust, TypeScript,
  TSX, JavaScript, Python, Markdown. Partial coverage (top-level
  keys only): JSON, YAML, TOML — the tool tells you to fall back
  to `rg` for those.

## Install

### Pre-built binaries (fastest)

Releases ship four targets per tag at
<https://github.com/mikolajmikolajczyk/repoctx/releases>:

| Target | Asset |
|---|---|
| Linux x86_64 | `repoctx-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `repoctx-<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `repoctx-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `repoctx-<version>-x86_64-pc-windows-msvc.zip` |

Verify the sha256 sidecar, unpack, drop the binary on `PATH`:

```sh
shasum -a 256 -c repoctx-0.5.1-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf repoctx-0.5.1-x86_64-unknown-linux-gnu.tar.gz
sudo mv repoctx-0.5.1-x86_64-unknown-linux-gnu/repoctx /usr/local/bin/
```

Full PowerShell and `curl` recipes per platform: [`wiki/user/installation.md`](wiki/user/installation.md).

### Cargo

```sh
cargo install --git https://github.com/mikolajmikolajczyk/repoctx --tag v0.5.1
```

### Nix

```sh
nix run github:mikolajmikolajczyk/repoctx
# or
nix profile install github:mikolajmikolajczyk/repoctx
```

## Wire it into your agent

`repoctx hook install <agent>` drops a skill + AGENTS.md guidance
into the current repo so your AI coding agent loads it automatically:

```sh
cd /path/to/your/project
repoctx hook install claude        # Claude Code
repoctx hook install codex         # OpenAI Codex
repoctx hook install opencode      # opencode
```

What it writes per agent:

- Claude → `.claude/skills/repoctx/SKILL.md` + a guidance block in
  `CLAUDE.md`.
- Codex / opencode → `.agents/skills/repoctx/SKILL.md` + a guidance
  block in `AGENTS.md`.

The skill file teaches the agent how to use `repoctx` and when to
prefer it over `rg`. Re-running `install` is a no-op when nothing has
changed. Full reference: [`wiki/user/hook.md`](wiki/user/hook.md).

## What's in the box

- `index` — incremental indexer (changes only reparse changed files).
- `symbols` — case-insensitive substring search; `--kind`, `--lang`,
  `--limit` filters.
- `outline` — symbol tree for one file (indented containment in human
  mode, flat in machine).
- `definition` — exact-name lookup, definition-kind whitelist (no
  struct-field noise).
- `context` — exact-name match + source window around each hit.
- `status` — counts, per-language breakdown, staleness.
- `languages` — coverage matrix; agents check this to decide when to
  fall back to `rg`.
- `config` — per-repo settings table (output format, gain recording,
  hook fetcher pin).
- `gain` — token-savings analytics ("navigation cost avoided").
- `hook` — per-agent install machinery.
- Three output formats over one set of typed records: human (TTY),
  TOON (pipes), JSON (`--json`).
- CI green on Linux + macOS + Windows.

Bench baseline on a 5,000-file synthetic corpus: cold index 318 ms,
no-op incremental 50 ms, warm `symbols` query 3 ms.

## Documentation

User docs under [`wiki/user/`](wiki/user/index.md):

- [Installation](wiki/user/installation.md) — Nix, Cargo, pre-built
  binaries with sha256 verification.
- [Quickstart](wiki/user/quickstart.md) — five-minute walk-through.
- [Commands reference](wiki/user/commands.md) — every flag, every
  exit code, the full kind vocabulary.
- [Hook — per-agent install](wiki/user/hook.md) — Claude Code /
  Codex / opencode integration.
- [Config — per-repo settings](wiki/user/config.md) — the settings
  table, precedence rules, env-var conventions.
- [Output formats + agent integration](wiki/user/output-formats.md) —
  TOON vs JSON vs human; CLAUDE.md recipe, jq snippets.
- [Gain analytics](wiki/user/gain.md) — what `gain` measures, baseline
  rules, privacy stance.

Agent docs (architecture, conventions, project status) under
[`wiki/agents/`](wiki/agents/) — start at [`AGENTS.md`](AGENTS.md).

## Contributing

Canonical forge is **Radicle**. The GitHub mirror exists for CI and
discoverability only — patches and issues there aren't monitored.

```sh
rad clone rad:z3ZAf4PfKZnuurn2YNz3t7cTLLUgB
cd repoctx
nix develop                            # pinned toolchain + tooling
rad issue list --all
git push rad HEAD:refs/patches         # submit a patch
```

If Radicle isn't your thing, open a GitHub issue describing what
you'd like to send and we'll figure it out.

## License

LGPL-3.0-or-later — see [`LICENSE`](LICENSE). Modifications to
`repoctx` itself stay open; integration into proprietary workflows
is fine.
