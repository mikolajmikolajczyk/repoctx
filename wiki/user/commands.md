# Commands reference

M0 + M1 surface: `index`, `symbols`, `status`, `outline`, `definition`, `context`, `gain`. All examples below were verified against the binary built from this repo on 2026-06-11.

## Global flags

| Flag | Effect |
|---|---|
| `--repo <PATH>` | Treat `<PATH>` as the search start (default: cwd). Repo root = nearest ancestor (incl. itself) containing `.git`, else the given dir. |
| `--json` | Force JSON output even on a TTY. Mutually exclusive with `--toon`. |
| `--toon` | Force [TOON](https://github.com/toon-format/toon) output even on a TTY. |
| `--no-record` | Skip gain analytics recording for this invocation. |
| `--record-query` | Persist the query string in the usage row (off by default). |
| `--no-auto-index` | Don't auto-index when a read command finds no index — bail with the `no index found` error instead. |
| `-v` / `-vv` | Verbosity: `-v` = info, `-vv` = debug. `RUST_LOG` overrides. |

Output format defaults: TTY → human; non-TTY → TOON; `--json` always JSON. See [`output-formats.md`](output-formats.md) and [ADR-0008](../adr/0008-toon-default-machine-output.md).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. **An empty result set is a success** — list commands print `count: 0` and exit 0. |
| `1` | Any error. Error message goes to stderr; machine stdout stays empty. |

Errors most users will see:

- `no index found — run 'repoctx index'` — only when `--no-auto-index` is set. Without that flag, a missing index causes a silent one-shot indexing pass on stderr before the query answers.
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
| TypeScript plain `class`/`function` | (not surfaced) | Upstream TS `tags.scm` covers only `interface_declaration`, `abstract_class_declaration`, and method signatures. Plain `class`/`function` are NOT tagged. |
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

Each item carries a `stale` flag. `stale: true` means the file's current `(mtime_ns, size)` no longer matches what the index recorded — likely the file was edited since the last `repoctx index`. The `body` and surrounding lines you're looking at may have shifted relative to the indexed `location`. Remedy: re-run `repoctx index` (or just any other read command, which auto-indexes) and retry.

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

## `repoctx hook`

Per-agent install machinery — drops the `repoctx` skill / guidance into a target repo so AI coding agents auto-load it. Three subcommands: `list`, `status`, `install`. No `uninstall` — `install` prints removal instructions on success. Full reference + per-agent table: [`hook.md`](hook.md).

| Subcommand | Effect |
|---|---|
| `repoctx hook list` | Enumerate available agents (`claude`, `codex`, `opencode`) with descriptions. |
| `repoctx hook status [--dir PATH]` | For each agent, show which destination files exist in the target dir. |
| `repoctx hook install <agent> [--dir PATH] [--dry-run] [--force] [--ref <git-ref>] [--no-cache]` | Install one agent's files. Idempotent re-install returns `skipped_identical`. |

Per-agent files are fetched at install time from GitHub raw at a pinned ref (default `v<binary version>`) and cached under XDG.

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
