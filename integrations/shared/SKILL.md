---
name: repoctx
description: Use repoctx for fast, indexed code navigation in this repo. Beats grep/find/cat on structural questions about symbols, definitions, the call graph (who-calls / callees / callgraph), and surrounding source.
---

# repoctx

`repoctx` is an AI-oriented repository intelligence CLI. The index is
built incrementally over Tree-sitter parses of 20 languages (Rust, Go,
Python, TypeScript, TSX, JavaScript, C, C++, Java, C#, Ruby, PHP, Lua,
Kotlin, Swift, Bash, Markdown, JSON, YAML, TOML). All read commands
auto-index on first run.

## When to use

Use this skill when the user's question is **structural**:

- "Where is X defined?"
- "What's the body of function Y?"
- "What public symbols does this file expose?"
- "Who calls function Z?" / "What does Z call?"
- "Trace the call chain from Z."
- "Are there any TODO-tagged sections?"
- "Show me every type that mentions Foo."

Do NOT reach for `grep`/`rg`/`find`/wholesale `Read` for these. Read
files when you need prose-level reasoning about implementation; reach
for `repoctx` to navigate.

## Commands

`{REPOCTX_BIN}` is the binary that installed this skill. All commands
default to TOON for piped reads and human for TTYs; pass `--json` when
parsing with `jq` or `serde_json`.

### `{REPOCTX_BIN} symbols <substring>`

Case-insensitive substring search across every indexed symbol. Narrow
with `--kind` (`function`, `method`, `class`, `interface`, `type`,
`module`, `macro`, `constant`, `variable`, `field`, `section`, `key`,
`other`) and `--lang` (`rust`, `go`, `typescript`, `tsx`, `javascript`,
`python`, `json`, `yaml`, `toml`, `markdown`). `--limit` caps results
(default 50; `0` = unlimited).

Use this for **exploration** when you don't know the exact identifier.

### `{REPOCTX_BIN} search <pattern>`

Textually-complete search: the symbol definitions named `<pattern>` **plus**
every textual occurrence ripgrep finds — comments, strings, config values,
anything `symbols` would miss — compressed to `file:line` (≤40 files, ≤8
matches/file). `--lang <slug>` restricts by language. This is what `rg
<ident>` rewrites to, so you usually get it automatically.

Use `search` when a mention in a comment/string/config might matter; use
`symbols`/`definition` when you only want the structural answer. Output:
`{pattern, symbols:[…], matches:{count, files:[{path, lines:[{line,text}]}]}}`.
An `advisory` fires on truncation or if ripgrep isn't installed.

### `{REPOCTX_BIN} definition <name>`

Exact-name (case-sensitive) lookup. Auto-filters to definition kinds —
field/variable/section/key noise is excluded. `--lang`/`--limit` apply.
Multiple hits are normal; pick the right one. Zero hits returns exit 0
with `count: 0` — but because the match is case-sensitive, a 0-hit may
carry an `advisory` naming case-insensitive near-misses (e.g. you typed
`store`, `Store` exists). Retry with the exact casing it suggests.

Use this when you **know the name** and want canonical definition
sites.

### `{REPOCTX_BIN} context <symbol> [--context C] [--limit N]`

Exact-name match + source window. Defaults: 5 lines above/below, top 3
hits. Reads source from disk so the bytes are current; `stale: true`
flags hits whose indexed `(mtime_ns, size)` no longer matches disk.

Prefer this over `definition` + a follow-up file Read — one round trip
instead of two.

### `{REPOCTX_BIN} callers <name>` / `callees <name>`

Direct call-graph edges. `callers` = who calls `name`; `callees` = what
`name` calls. `--limit` caps results (default 50). Each edge gives the
caller symbol, the callee (resolved symbol, or `null` when
external/unresolved), the call site, and an `ambiguous` flag.

Use these instead of `rg "name("` — you get structured caller/callee
symbols with locations, not raw text matches.

### `{REPOCTX_BIN} callgraph <name> [--depth N] [--direction up|down|both]`

Transitive call graph from `name`. `--depth` (default 3) bounds the walk;
`--direction down` follows callees, `up` follows callers, `both` does
both. Each edge is tagged with `depth` and `direction`. Cycle-safe.

**Accuracy — read before trusting it.** The call graph is name-based and
approximate, the same accuracy class as `definition`:

- No receiver-type disambiguation — `a.foo()` and `b.foo()` both resolve
  to every `foo` (such edges are flagged `ambiguous: true`).
- External / stdlib / dynamically-dispatched callees show `callee: null`
  (name shown, location unknown).
- Function pointers, higher-order calls, and cross-language edges are
  invisible.
- Languages: the core 8 — Rust, Python, JavaScript, TypeScript, Go, C,
  C++, Java. Other languages return no edges yet.

When edges are ambiguous or unresolved the command emits an `advisory`;
treat the graph as a strong hint and cross-check with `rg` when it
matters.

### `{REPOCTX_BIN} outline <file>`

Document-symbol tree for one file. Path may be repo-relative or
absolute. Indented containment tree (human) or flat `{count, items}`
(machine).

Use this when you need the **structure** of a file but not its full
contents.

### `{REPOCTX_BIN} status`

File/symbol counts + per-language breakdown + staleness. `--fast` skips
the staleness walk. Quick health check before deeper work.

### `{REPOCTX_BIN} gain`

Surface the navigation tokens this skill has actually saved. `gain top
--by saved` ranks per command.

## Output formats

| Flag | Format |
|---|---|
| (none, TTY) | Human (aligned columns) |
| (none, pipe) | TOON (token-efficient default) |
| `--json` | JSON |
| `--toon` | TOON forced on TTY |

`--json` and `--toon` are mutually exclusive.

## Empty results and errors

- An empty result set is exit 0 with `count: 0`. Check exit codes, not
  stderr strings.
- The index manages itself — read commands silently build the DB if
  needed and incrementally reindex changed files before answering.
- File arguments must be inside the repo root (`{REPO_ROOT}`).

## Transparent rewrite (it may already be happening)

If `repoctx hook install claude` ran on this repo, some of your
`rg` / `grep` commands get transparently rewritten to `repoctx`
equivalents before Claude Code executes them. You don't need to do
anything — the rewrite is automatic for these patterns:

- `rg <identifier>` → `repoctx search <identifier> --json`
  (textually complete: symbol defs **+** every ripgrep match — no loss)
- `rg "fn <name>"` / `class` / `struct` / `function` →
  `repoctx definition <name> --json`
- **Navigation flags on a bare identifier are understood** (they change
  rg's output, not the intent):
  - `rg -n` / `-l` / `-i` / `-w` / `-F <ident>` → `repoctx search`
  - `rg --type <lang> <ident>` → `repoctx search <ident> --lang <lang>`
  - `rg -A/-B/-C <n> <ident>` → `repoctx context <ident> --context <n>`
- Same shapes for `grep -r` / `grep -rn`

There is **no** transparent rewrite for the call graph — `rg "name("`
stays a literal search (the `(` is a regex metacharacter). Call
`callers` / `callees` / `callgraph` explicitly when you want call edges.

Hard passthrough (your command runs as-is): regex patterns, shell
metacharacters, multiple identifiers, explicit paths, single-quoted
literals (`rg "TODO"` stays a literal grep), and non-navigation flags
that change the result set rather than its formatting (`-c` count, `-v`
invert, `-o` only-matching, `--json`, unknown `--type`). Any
other PreToolUse hook the user had installed (rtk, etc.) runs as
a chain after our rewrite layer — so all your other commands still
get whatever optimizations were there before.

You'll see the rewrite reflected in the tool input you receive; if
you want to bypass it for one command, use a pattern that doesn't
match the rules (e.g. quote it, add a flag, or use a non-`.`
path).

## Coverage advisory — when to fall back to ripgrep

Every machine output can carry an optional top-level `advisory`
field. When present, it means repoctx may be underserving this
query because of language coverage limits. The advisory text
suggests a concrete fallback command, typically `rg -n <pattern>`.

Always check `advisory` on your responses. When set, also run the
suggested `rg` command and merge the results — don't trust
repoctx's `count: 0` blindly on a partial-coverage language.

To get the full coverage matrix in one call:

```sh
{REPOCTX_BIN} languages --json
```

Returns `{slug, coverage: "full"|"partial", notes}` per language.
Cache this once per session.

Today (v0.8.0): 16 languages are `full` — Rust, Go, Python, TypeScript,
TSX, JavaScript, C, C++, Java, C#, Ruby, PHP, Lua, Kotlin, Swift,
Markdown. JSON, YAML, TOML are `partial` (top-level keys / TOML section
headers; opt-in all-depth via `index.nested_keys`), and Bash is
`partial` (functions only). A query like "where is `containerPort`
configured" against a k8s YAML will return zero hits even though `rg`
would find it — but `repoctx search containerPort` will, since it runs
ripgrep too. The advisory
will tell you so.

## Gotchas

- Rust `struct`/`enum`/`union`/`type` are all reported as `class` per
  the upstream `tags.scm` mapping; Rust `trait` is `interface`.
- TypeScript and TSX have full coverage including plain class, plain
  function, arrow functions assigned to identifiers, type aliases,
  and enums. (Vendored from Aider, Apache-2.0.)
- Markdown headings (ATX and setext) are reported as `section`.
- Top-level JSON/YAML/TOML keys are `key`; nested keys are not
  surfaced. The advisory field will warn you when this matters.

## Examples

```sh
# Find every type alias that mentions "Token"
{REPOCTX_BIN} symbols Token --kind type --json | jq '.items[]'

# Where is `parse_config` defined?
{REPOCTX_BIN} definition parse_config

# Who calls `parse_config`, and what does it call?
{REPOCTX_BIN} callers parse_config
{REPOCTX_BIN} callees parse_config

# Trace two hops of what `handle_request` calls
{REPOCTX_BIN} callgraph handle_request --depth 2 --direction down

# Show me the resolve_path function with 10 lines of context
{REPOCTX_BIN} context resolve_path --context 10 --limit 1

# What's the structure of the main entry point?
{REPOCTX_BIN} outline src/main.rs

# How healthy is the index?
{REPOCTX_BIN} status
```
