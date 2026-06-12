# Config schema, storage, and precedence

**Date**: 2026-06-12. **Issue**: `bbf9070`. **Epic**: `2c96964`.

> **Superseded in part (0.5.3):** the `hook.ref` and `hook.no_cache` keys
> were removed when integration content moved into the binary (issue
> `43aeaff`). The schema, storage, and precedence model below still hold;
> ignore those two keys. Live key list: [`wiki/user/config.md`](../user/config.md).

## What

Pin the schema + storage backend + precedence rules for the
per-repo config system landing in v0.4.0. Subsequent children
implement against this doc.

## Decisions

### Storage

Per-repo SQLite. New `settings` table inside the existing
`.repoctx/index.db`. **No** global config file (`~/.config/`,
XDG, dotfiles) — revisit only when a real use case turns up.

```sql
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

Schema version bumps from **2 → 3** with the migration appended
to `crates/store/src/migrations.rs`. Empty `settings` is the
default state — every key absent → built-in default applies.

The settings table doesn't store types — values are TEXT and
the loader parses to typed values. This keeps the migration
trivial and lets us extend the schema for future keys without
touching SQL.

### Key namespacing

Dotted strings: `hook.rewrite`, `gain.no_record`, `output.default`.
Stored verbatim. The dot is purely a presentation convention; the
loader doesn't parse hierarchy out of it.

This matches what TOML files would look like if we ever add a
file-based layer, makes env-var translation mechanical
(`REPOCTX_<UPPER_DOTS_TO_UNDERSCORES>`), and reads naturally on
the CLI (`config set hook.rewrite off`).

### Initial key set

| Key | Type | Values | Default | Notes |
|---|---|---|---|---|
| `hook.rewrite` | enum | `auto` \| `off` \| `force` | `auto` | rewrite-hook kill switch (consumer in v0.5.0) |
| `hook.ref` | string | git ref | `v<binary version>` | fetcher pin for `hook list/status/install` |
| `hook.no_cache` | bool | `true` \| `false` | `false` | bypass XDG cache for hook fetches |
| `gain.no_record` | bool | `true` \| `false` | `false` | persistent `--no-record` |
| `gain.record_query` | bool | `true` \| `false` | `false` | persistent `--record-query` |
| `output.default` | enum | `auto` \| `human` \| `toon` \| `json` | `auto` | persistent output-format choice |

Booleans accept `true` / `false` / `1` / `0` / `yes` / `no` (case
insensitive) on **read**. Writes via `config set` normalize to
`true` / `false`.

Enum values are case-insensitive on read, lowercased on write.

### Precedence (highest wins)

1. CLI flag on this invocation.
2. Environment variable.
3. `settings` row.
4. Built-in default.

The `Config` struct surfaces the resolved value AND a `Source`
enum (`Cli` | `Env` | `Settings` | `Default`) so `config show` can
annotate each line with where the value came from. Useful for
debugging.

### Env var naming

`REPOCTX_<SECTION>_<KEY>` in screaming snake. Dots become
underscores. Examples:

- `REPOCTX_HOOK_REWRITE=off`
- `REPOCTX_GAIN_NO_RECORD=1`
- `REPOCTX_OUTPUT_DEFAULT=json`

Existing `RUST_REPOCTX_NO_RECORD` is kept as a deprecated alias
for back-compat. Deprecation note in the gain consumer's stderr
emission and CHANGELOG.

### Validation

Two layers:

- **Write-time** (`config set`, programmatic `Store::set_setting`):
  enum keys reject unknown values up front with a clear error
  message naming the legal set. Bool keys reject non-bool inputs.
  Strings (e.g. `hook.ref`) are stored as-is.
- **Read-time** (loader): if a stored value fails to parse
  (e.g. someone hand-edited the DB), log a `WARN` on stderr
  naming the key + value + reason, fall back to default. Better
  than crashing — config drift across binary versions shouldn't
  brick read commands.

Unknown KEYS at either layer are warned but accepted. This keeps
older binaries usable against settings tables written by newer
binaries that learned new keys.

### Schema migration v2 → v3

Single statement:

```sql
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

Idempotent. Runs under the existing `BEGIN IMMEDIATE` migration
guard so parallel indexers serialize cleanly (existing test
covers this; settings adds nothing new).

### CLI surface

```
repoctx config show              # every effective key + source
repoctx config get <key>         # one value
repoctx config set <key> <value> # validate + write
repoctx config unset <key>       # delete row (fall back to default)
```

`show` machine output: `{count, items: [{key, value, default, source}]}`.
Human output: aligned columns.

`set` exits 1 with a precise error when validation rejects:
`hook.rewrite must be one of [auto, off, force] (got 'banana')`.

### Source enum

```rust
pub enum Source {
    Cli,        // ad-hoc CLI flag
    Env,        // env var
    Settings,   // settings row
    Default,    // built-in
}
```

CLI consumers layer their override on top of the loaded `Config`
by mutating the field and bumping `Source::Cli`. The loader
doesn't know about CLI flags directly — keeps the loader pure.

## Migration of existing flags

| Existing flag | Config key | Status |
|---|---|---|
| `--no-record` | `gain.no_record` | one-shot CLI flag stays; config-backed default is new |
| `--record-query` | `gain.record_query` | same |
| `--json` / `--toon` | `output.default` | one-shot flag stays; persistent default is new |
| `--no-cache` (on `hook list/status/install`) | `hook.no_cache` | same |
| `--ref` (on `hook list/status/install`) | `hook.ref` | same |
| `--no-rewrite` (future, `hook claude`) | `hook.rewrite` | same |
| `--verbose` / `-v` | NOT migrated | per-invocation only; no use case for persistence |
| `--json` mutual exclusion with `--toon` | unchanged | clap still enforces |

## Out of scope

- Global config (`~/.config/repoctx/`). Per-repo only. Revisit
  on real demand.
- TOML config files. Settings live in SQLite.
- Per-language config (`config index.deep.yaml`). Tracked in
  the nested-keys epic.
- Hot reload. Each invocation reads fresh.
- Secret storage. repoctx is fully local; no API keys to keep.

## References

- Issue `2c96964` (epic).
- Issue `29d6186` (loader implementation, blocked on this).
- Issue `1a19873` (kind rename, deferred — same `wiki/decisions/`
  surface but unrelated decision).
