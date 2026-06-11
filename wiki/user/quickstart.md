# Quickstart

Five minutes from a working `repoctx` install to indexed search. The walkthrough below was captured against this repo on 2026-06-11.

## 0. Install

See [`installation.md`](installation.md). The rest of this page assumes `repoctx --help` works.

## 1. Index a repository

`cd` into the root of any Git repository. You can run `repoctx index` explicitly:

```sh
repoctx index
```

```text
indexed 81 files (0 unchanged, 0 removed) in 69 ms
```

…but you don't have to. `symbols` / `outline` / `definition` / `context` run an incremental reindex automatically before answering — first run builds the DB, every later run cheaply reparses only the files whose `(mtime, size)` tuple changed. `status` and `gain` only auto-build a missing DB (they never auto-reindex on top of an existing one). Pass `--no-auto-index` to opt out entirely.

What just happened:

- Tree-sitter parsed every supported file (Go, Rust, TypeScript, JavaScript, Python, JSON, YAML, TOML, Markdown).
- Symbols, file mtimes, and sizes landed in `.repoctx/index.db` (a SQLite file at the repo root).
- Files larger than 2 MiB, files that aren't UTF-8, and anything matching `.gitignore` were skipped.

**Add `.repoctx/` to `.gitignore`** in any repo you index so the database doesn't follow you into commits:

```sh
echo ".repoctx/" >> .gitignore
```

Re-running `repoctx index` is cheap — only files whose `(mtime, size)` tuple changed are reparsed:

```sh
repoctx index
```

```text
indexed 0 files (81 unchanged, 0 removed) in 2 ms
```

`--force` reparses everything.

## 2. Search symbols

```sh
repoctx symbols main --limit 3
```

```text
crates/repoctx/src/main.rs:130  main  function
```

Substring search is case-insensitive. Filter with `--kind` (`function`, `class`, `section`, …) or `--lang` (`rust`, `go`, `markdown`, …). Empty result is a clean exit 0 with `count: 0`.

```sh
repoctx symbols Cat --kind class --lang typescript
```

## 3. Check index health

```sh
repoctx status
```

```text
schema_version: 2
files:          81
symbols:        528
db_size_bytes:  131072
per_language:
  json         2
  markdown     33
  rust         39
  toml         5
  yaml         2
staleness:      changed=0 new=0 deleted=0
```

The `staleness` line tells you whether the index is up to date with the working tree (`changed` = edited files, `new` = files appeared since the last index, `deleted` = indexed files that vanished). Pass `--fast` to skip the staleness walk when you only need counts.

## 4. Pull a symbol with its surrounding source

The flagship agent query — definition site plus a code window, in one call:

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
  245      match since {
  246          Some(s) => Ok(gain_cmd::Window::Since(gain_cmd::parse_since(s)?)),
  247          None => Ok(gain_cmd::default_window()),
  248      }
  249  }
```

Need the file's full structure instead? `repoctx outline crates/repoctx/src/main.rs` prints the indented symbol tree. Want only canonical definitions, no substring noise? `repoctx definition resolve_window` returns exact-name hits.

## 5. See what `repoctx` saved you

`repoctx` records one row per read command (default on; turn off per-invocation with `--no-record` or per shell with `RUST_REPOCTX_NO_RECORD=1`). After a few queries:

```sh
repoctx gain
```

```text
Last 30 days

Commands:
  1

Returned:
  15 tokens

Estimated baseline:
  1,257 tokens

Reduction:
  98.8%

Estimated savings:
  1.2K tokens
```

That's the navigation cost an agent did NOT have to pay because `repoctx` answered with a narrow result instead of forcing it to grep + open whole files. See [`gain.md`](gain.md) for the philosophy and privacy stance.

## What's next

- Full reference for every flag: [`commands.md`](commands.md).
- Switching between human, TOON, and JSON output (and wiring it into AI agents): [`output-formats.md`](output-formats.md).
