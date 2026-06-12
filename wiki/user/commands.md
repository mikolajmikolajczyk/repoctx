# Commands reference

Ten commands: `index`, `symbols`, `outline`, `definition`, `context`, `status`, `languages`, `config`, `hook`, `gain`. All examples below were verified against the v0.4.0 binary on 2026-06-12.

## Global flags

| Flag | Effect |
|---|---|
| `--repo <PATH>` | Treat `<PATH>` as the search start (default: cwd). Repo root = nearest ancestor (incl. itself) containing `.git`, else the given dir. |
| `--json` | Force JSON output even on a TTY. Mutually exclusive with `--toon`. |
| `--toon` | Force [TOON](https://github.com/toon-format/toon) output even on a TTY. |
| `--no-record` | Skip gain analytics recording for this invocation. |
| `--record-query` | Persist the query string in the usage row (off by default). |
| `-v` / `-vv` | Verbosity: `-v` = info, `-vv` = debug. `RUST_LOG` overrides. |

Output format defaults: TTY → human; non-TTY → TOON; `--json` always JSON. See [`output-formats.md`](output-formats.md) and [ADR-0008](../adr/0008-toon-default-machine-output.md).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. **An empty result set is a success** — list commands print `count: 0` and exit 0. |
| `1` | Any error. Error message goes to stderr; machine stdout stays empty. |

Errors most users will see:

- Missing-index errors don't exist for users — read commands always build the DB if needed and run an incremental reindex (one short stderr line when work happened, otherwise silent) before answering.
- `index is locked by another repoctx process — retry` — two writers raced for longer than the 5 s busy timeout.
- `index is corrupted — delete .repoctx/ and re-run 'repoctx index'` — the DB file is non-SQLite or corrupted.
- `index was created by a newer repoctx — upgrade repoctx or delete .repoctx/` — schema version is ahead of this binary.

## `repoctx index`

Walk the repository, parse changed files in parallel, write through a single sequential SQLite writer, prune absent files, print a one-line summary.

| Flag | Effect |
|---|---|
| `--force` | Reparse every file, even those with an unchanged `(mtime, size)` tuple. |

What is **always** skipped: `.git/`, `.repoctx/`, files matched by `.gitignore`/`.git/info/exclude`/the user's global gitignore, files larger than **2 MiB**, files that don't decode as UTF-8. Skipped files get a single `WARN` line on stderr each.

Example (machine summary via `--json`):

```sh
repoctx index --json
```

```json
{"indexed":81,"unchanged":0,"removed":0,"duration_ms":69}
```

## `repoctx symbols <query>`

Case-insensitive substring search across every indexed symbol.

| Flag | Effect |
|---|---|
| `--kind <kind>` | Restrict to one kind (see the table below). |
| `--lang <slug>` | Restrict to one language slug (`rust`, `go`, `typescript`, `tsx`, `javascript`, `python`, `json`, `yaml`, `toml`, `markdown`). |
| `--limit <N>` | Cap results at `N` (default `50`; `0` = unlimited). |

Result ordering is deterministic: `name COLLATE NOCASE ASC`, then `file_path ASC`, then `start_line ASC`.

### Kind vocabulary

`repoctx` reuses each Tree-sitter grammar's upstream `tags.scm` as-is (ADR-0002). The full enum surfaced by `--kind` is:

```text
function method class struct enum interface trait module
constant type variable field macro section key other
```

**Some upstream quirks worth knowing:**

| Source code | Reported kind | Why |
|---|---|---|
| Rust `struct`, `enum`, `union`, `type` | `class` | Upstream Rust `tags.scm` collapses them all to `@definition.class`. |
| Rust `trait` | `interface` | Same upstream choice. |
| Go `type X struct {}` / `type Y interface {}` | `type` | Upstream Go `tags.scm` uses `@definition.type`. |
| TypeScript / TSX `class`, `function`, arrow `const X = () => ...`, `type`, `enum` | tagged (vendored Aider `tags.scm`, Apache-2.0) | repoctx ships a richer query than upstream `tree-sitter-typescript`. Covers plain class, plain function, arrow-function assigned to identifier, type aliases, enums, interfaces, and abstract classes. |
| Markdown headings | `section` | Custom query — ATX (`#`) and setext (`===`/`---`) headings. |
| JSON/YAML/TOML top-level keys | `key` | Custom query — root-level keys only; nested keys not surfaced. |

Filter and tweak as needed: `--kind class --lang rust` to walk every Rust struct/enum/type alias, `--kind section --lang markdown` for a table-of-contents view, etc.

## `repoctx outline <file>`

Document-symbol view of a single file. Human mode prints an indented containment tree (a method's range sits inside its impl's range, so the method nests). Machine mode is a flat `{count, items}` list with full 0-based ranges, so downstream tools don't have to reconstruct the tree.

Path argument accepts both forms:

- Repo-relative: `repoctx outline crates/repoctx/src/main.rs`
- Absolute: `repoctx outline /full/path/to/main.rs`

Both are canonicalized and re-anchored against the repo root before lookup. A path outside the repo bails with `path is outside repo: …`.

If the file isn't in the index, you get:

```text
crates/foo/bar.rs is not in the index — file may be new, ignored, oversized
(>2 MiB), non-UTF-8, or in an unsupported language. Run `repoctx index` to refresh.
```

Exit 1. Those four causes are the entire set — no other reason exists for a file to be missing once `index` succeeds.

```sh
repoctx outline --json crates/repoctx/src/outline_cmd.rs
```

```json
{"count":7,"items":[{"name":"OutlineReport","kind":"class","location":{"path":"crates/repoctx/src/outline_cmd.rs","start_line":15,"start_column":0,"end_line":20,"end_column":1}}, …]}
```

## `repoctx definition <name>`

Exact-name (case-sensitive) lookup. **Contrast with `symbols`**: `symbols foo` matches any name containing `foo` (substring, case-insensitive); `definition foo` matches only names that are exactly `foo`. Use `definition` when you know the identifier and want the canonical site; use `symbols` to explore.

| Flag | Effect |
|---|---|
| `--lang <slug>` | Restrict to one language slug. |
| `--limit <N>` | Cap results at `N` (default `50`). |

The kind filter is fixed — **only** these kinds can be a "definition":

```text
function method class interface type module macro constant
```

Variables, fields, sections, keys, and `other` are excluded so a search for `path` doesn't drown in struct-field hits. Rust `struct`/`enum`/`trait` reach this set via upstream `tags.scm` mapping (see the kind quirks table under `symbols`).

Multiple hits are normal (think `run` defined in many modules) — the command lists all of them; the agent picks. Zero hits is a clean exit 0 with `count: 0`.

```sh
repoctx definition --json main --lang rust
```

```json
{"count":1,"items":[{"name":"main","kind":"function","location":{"path":"crates/repoctx/src/main.rs","start_line":161,"start_column":0,"end_line":238,"end_column":1}}]}
```

## `repoctx context <symbol>`

The flagship agent query: "where is X defined AND what does it look like?". One call returns every exact-name match plus a window of surrounding source.

| Flag | Effect |
|---|---|
| `--context <C>` | Lines of leading and trailing context. Default `5`. `0` returns body only. |
| `--limit <N>` | Maximum number of matches. Default `3`. |

Matching: exact name (case-sensitive), any kind. Ranking when there are more hits than `--limit`: shorter file path first, then `(start_line, start_column)`. So a top-level `main` in `src/main.rs` ranks above a vendored copy in `vendor/lib/src/main.rs`.

For each match, the source window is read **from disk** (not the DB) so you get current bytes. The window is `start_line - C .. end_line + C`, clamped to file bounds (top-of-file → empty `before`, bottom-of-file → empty `after`).

Each item carries a `stale` flag. Effectively always `false` in normal use: `context` auto-reindexes changed files before answering, so the indexed `location` matches what's on disk. The flag survives in the schema for edge cases (concurrent file edits, files modified between the reindex and the source read).

If the file was deleted since indexing, the match is dropped with a `WARN` line on stderr and remaining matches still print.

Machine shape per match:

```json
{
  "symbol": "main",
  "kind": "function",
  "location": {"path": "crates/repoctx/src/main.rs",
               "start_line": 161, "start_column": 0,
               "end_line": 238, "end_column": 1},
  "before": "…",
  "body": "fn main() -> Result<()> {\n    let cli = Cli::parse();\n    …\n}",
  "after": "…",
  "stale": false
}
```

Wrapper: `{count, items}`. Human mode prints `# path:line  name  kind` per match, an optional `(stale …)` line, then a numbered listing.

```sh
repoctx context resolve_window --context 2 --limit 1
```

```text
# crates/repoctx/src/main.rs:241  resolve_window  function
  239  }
  240
  241  fn resolve_window(since: Option<&str>, all: bool) -> Result<gain_cmd::Window> {
  242      if all {
  243          return Ok(gain_cmd::Window::All);
  244      }
  …
```

## `repoctx status`

Index health + per-language counts + optional staleness.

| Flag | Effect |
|---|---|
| `--fast` | Skip the staleness stat-walk (counts only). |

Output fields:

| Field | Meaning |
|---|---|
| `schema_version` | DB schema version (currently `2`). |
| `files` | Total indexed files. |
| `symbols` | Total symbols across all files. |
| `db_size_bytes` | On-disk size of `.repoctx/index.db`. |
| `per_language` | List of `{ language, files }` rows, alphabetical. |
| `staleness.changed` | Files whose `(mtime, size)` tuple no longer matches the index. |
| `staleness.new` | Files that exist on disk but aren't in the index yet. |
| `staleness.deleted` | Files that are in the index but no longer on disk. |

`--fast` drops the entire `staleness` block.

## `repoctx languages`

Print the per-language coverage matrix repoctx uses to advise agents on when to fall back to `ripgrep`. No arguments, no flags.

```sh
repoctx languages
```

```text
rust        full     struct/enum/union/type → class, trait → interface (upstream tags.scm)
go          full     func / method / type (struct/interface) (upstream tags.scm)
python      full     def / class (upstream tags.scm)
typescript  full     interface, class, function (incl. arrow), method, type, enum (vendored Aider tags.scm)
tsx         full     same coverage as TypeScript (vendored Aider tags.scm)
javascript  full     class, function (incl. arrow), method (upstream tags.scm)
markdown    full     ATX (#) and setext headings (custom query)
toml        partial  root pairs + [table] + [[array]] headers; keys inside tables are not surfaced
json        partial  top-level keys only; nested keys are not surfaced
yaml        partial  top-level keys of each document; nested keys are not surfaced
```

`coverage` is `full` or `partial`. Read commands attach an `advisory` field to their machine output when the query targets a `partial`-coverage language (or the workspace contains files in one and the query returned zero hits). The advisory text suggests a concrete `rg -n` fallback.

## Coverage advisory on read commands

`outline`, `definition`, `context`, and `symbols` may include an `advisory` field in their machine output. Always omitted in the happy path; present when:

- The target file's language is `partial` (`outline` over a YAML/JSON/TOML file).
- `--lang <slug>` was supplied and that slug is `partial`.
- `count == 0` and the workspace has at least one file in a `partial` language.

Human render appends a final `advisory: <text>` line. Machine renders include `"advisory": "..."`.

Agents should treat a non-null `advisory` as a hint to also run the suggested `rg` command and merge the results, rather than trusting `count: 0` as authoritative.

## `repoctx config`

Per-repo settings table (lives in `.repoctx/index.db`). Four
subcommands. Full reference + key schema + env-var naming + precedence
rules: [`config.md`](config.md).

| Subcommand | Effect |
|---|---|
| `repoctx config show` | Every effective key + its current value + source (`cli` / `env` / `settings` / `default`). |
| `repoctx config get <key>` | One value, with its source. |
| `repoctx config set <key> <value>` | Validate + write. Rejects unknown keys and out-of-range values. |
| `repoctx config unset <key>` | Delete row; built-in default applies again. |

Precedence (highest wins): CLI flag → environment variable → settings
row → built-in default. Keys today: `hook.rewrite`,
`hook.chain_commands`, `gain.no_record`, `gain.record_query`,
`output.default`.

## `repoctx hook`

Per-agent install machinery — drops the `repoctx` skill / guidance into a target repo so AI coding agents auto-load it. Three subcommands: `list`, `status`, `install`. No `uninstall` — `install` prints removal instructions on success. Full reference + per-agent table: [`hook.md`](hook.md).

| Subcommand | Effect |
|---|---|
| `repoctx hook list` | Enumerate available agents (`claude`, `codex`, `opencode`) with descriptions. |
| `repoctx hook status [--dir PATH]` | For each agent, show which destination files exist in the target dir. |
| `repoctx hook install <agent> [--dir PATH] [--dry-run] [--force]` | Install one agent's files. Idempotent re-install returns `skipped_identical`. |

Per-agent files are embedded in the binary — install works offline and always matches your installed version.

## `repoctx gain`

Surface the navigation cost the agent avoided. `gain` defaults to the **last 30 days**; the subcommand `gain top` ranks per command.

| Flag | Effect |
|---|---|
| `--since <window>` | Override the window. `7d`, `2h`, `30m`, `120s`. |
| `--all` | Drop the window — all-time totals. |
| `--history [N]` | Swap the summary for the N most recent rows (default `20`). |

### `repoctx gain top`

| Flag | Effect |
|---|---|
| `--by saved` | (default) Rank by absolute `estimated_savings`. Tiebreak on command name. |
| `--by ratio` | Rank by `reduction` percentage instead. |
| `--since`/`--all` | Same semantics as on `gain`. |

`gain` invocations are **not** themselves recorded. Empty usage in the window is a success: zeros, exit 0.

Full philosophy + privacy stance: [`gain.md`](gain.md).
