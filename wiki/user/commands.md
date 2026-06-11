# Commands reference

The M0 surface: `index`, `symbols`, `status`, `gain`. All examples below were verified against the binary built from this repo on 2026-06-11.

> M1 commands (`outline`, `definition`, `context`) land with Radicle issue `38865bb`.

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

- `no index found — run 'repoctx index'` — any read command on a directory that hasn't been indexed yet.
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
