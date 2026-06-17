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

Full per-query numbers, on three SHA-pinned real codebases (helix,
vuejs/core, rust-analyzer) and refreshed each release, live on the
[benchmark results page](wiki/bench/results.md). The method is documented
in [why repoctx saves tokens](wiki/user/why-repoctx.md).

## What it does

`repoctx` indexes your repo once into a tiny SQLite database (under
`.repoctx/index.db`), then answers structural questions about your
code in milliseconds:

- "Where is `X` defined?"
- "Show me `X` with the surrounding 5 lines."
- "What's the symbol structure of this file?"
- "Which functions / classes / types match this pattern?"
- "Who calls `X`? What does `X` call? Trace the call chain."
- "Every occurrence of `X` — definitions *and* textual mentions."

It's **structural-first**: each answer comes with the exact file, line,
column, and symbol kind, so the agent receives a tiny structured response
instead of a wall of source to grep through. `repoctx search` adds a
textually-complete mode — symbol definitions **plus** every ripgrep match
(comments, strings, anything), compressed — so you never lose textual data
either.

### Example queries

```sh
repoctx symbols UserService            # find symbols matching a substring
repoctx search parse_config            # symbol defs + every textual match
repoctx definition parse_config        # exact-name lookup ("where is X")
repoctx context resolve_window         # "show me X with code around it"
repoctx callers parse_config           # who calls it
repoctx callees parse_config           # what it calls
repoctx callgraph parse_config --depth 2 --direction up   # trace the chain
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
- 20 languages indexed. Full coverage: Rust, Go, Python, TypeScript,
  TSX, JavaScript, C, C++, Java, C#, Ruby, PHP, Lua, Kotlin, Swift,
  Markdown. Partial: JSON, YAML, TOML (top-level keys), Bash
  (functions) — the tool tells you to fall back to `rg` for those.
- A static **call graph** (`callers`/`callees`/`callgraph`) for the
  core 8 languages (Rust, Python, JS, TS, Go, C, C++, Java), built
  from Tree-sitter syntax — name-based, the same accuracy class as
  `definition` (ADR-0010).

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
shasum -a 256 -c repoctx-0.8.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf repoctx-0.8.0-x86_64-unknown-linux-gnu.tar.gz
sudo mv repoctx-0.8.0-x86_64-unknown-linux-gnu/repoctx /usr/local/bin/
```

Full PowerShell and `curl` recipes per platform: [`wiki/user/installation.md`](wiki/user/installation.md).

### Cargo

```sh
cargo install --git https://github.com/mikolajmikolajczyk/repoctx --tag v0.8.0
```

### Nix

```sh
nix run github:mikolajmikolajczyk/repoctx
# or
nix profile install github:mikolajmikolajczyk/repoctx
```

## Wire it into your agent

One command does everything. For **Claude Code** it installs the skill +
`CLAUDE.md` guidance and wires a **SessionStart** hook that runs
`repoctx prime`, so every new session begins with a compact repo
orientation digest in context and the agent reaches for repoctx instead
of blind `grep`/`cat`:

```sh
cd /path/to/your/project
repoctx init        # add -g for a user-global install (all repos)
```

It adds a `SessionStart` hook entry to `.claude/settings.json` and drops
`.claude/skills/repoctx/SKILL.md` + a `CLAUDE.md` block. Adoption is via
session-start **priming**, not command interception — repoctx doesn't
touch `PreToolUse`, so your own `rtk` (or other) hook runs independently.
Full reference: [`wiki/user/init.md`](wiki/user/init.md).

For **Codex / opencode** (rules-only agents), install just the guidance:

```sh
repoctx init --agent codex         # OpenAI Codex
repoctx init --agent opencode      # opencode
```

These write `.agents/skills/repoctx/SKILL.md` + an `AGENTS.md` block. The
skill teaches the agent how to use `repoctx` and when to prefer it over
`rg`. Re-running is a no-op when nothing changed.

## What's in the box

- `index` — incremental indexer (changes only reparse changed files).
- `symbols` — case-insensitive substring search; `--kind`, `--lang`,
  `--limit` filters.
- `search` — textually-complete search: symbol defs + every ripgrep
  match, compressed.
- `outline` — symbol tree for one file (indented containment in human
  mode, flat in machine).
- `definition` — exact-name lookup, definition-kind whitelist (no
  struct-field noise).
- `context` — exact-name match + source window around each hit.
- `callers` / `callees` — direct static call-graph edges.
- `callgraph` — transitive call paths (`--depth`, `--direction`;
  cycle-safe).
- `status` — counts, per-language breakdown, staleness.
- `languages` — coverage matrix; agents check this to decide when to
  fall back to `rg`.
- `config` — per-repo settings table (output format, gain recording,
  nested keys, subsystem size).
- `gain` — token-savings analytics ("navigation cost avoided").
- `prime` — compact ~600-token session-start orientation digest; what the
  SessionStart hook injects into the agent's context.
- `init` — wire repoctx into your agent: guidance files + (for Claude) a
  SessionStart hook that runs `repoctx prime`. `--uninstall` reverses it.
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
- [Onboarding — `repoctx init`](wiki/user/init.md) — Claude Code /
  Codex / opencode integration (guidance + SessionStart prime).
- [Config — per-repo settings](wiki/user/config.md) — the settings
  table, precedence rules, env-var conventions.
- [Output formats + agent integration](wiki/user/output-formats.md) —
  TOON vs JSON vs human; CLAUDE.md recipe, jq snippets.
- [Gain analytics](wiki/user/gain.md) — what `gain` measures, baseline
  rules, privacy stance.
- [Why repoctx saves tokens](wiki/user/why-repoctx.md) — the cost model,
  with a link to the per-release [benchmark results](wiki/bench/results.md).

Agent docs (architecture, conventions, project status) under
[`wiki/agents/`](wiki/agents/) — start at [`AGENTS.md`](AGENTS.md).

## Contributing

Code lives on **GitHub** — <https://github.com/mikolajmikolajczyk/repoctx>.
GitHub is the canonical forge: code, pull requests, **and** issues. Open a
pull request for any change; track work and roadmap in GitHub issues.

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
nix develop                            # pinned toolchain + tooling
# branch, commit, then open a PR on GitHub
gh pr create
```

Browse the roadmap and open work with `gh issue list --all`.

## License

LGPL-3.0-or-later — see [`LICENSE`](LICENSE). Modifications to
`repoctx` itself stay open; integration into proprietary workflows
is fine.
