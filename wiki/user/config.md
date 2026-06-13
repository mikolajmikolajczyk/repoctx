# Config

`repoctx` keeps per-repo behavior in a tiny key-value table inside the
existing `.repoctx/index.db` SQLite database. No `~/.config/`, no XDG
files, no TOML — every repo carries its own settings, and there's no
global cross-repo state.

## How precedence works

For every config-backed value, repoctx layers four sources. Highest
wins.

1. **CLI flag** on this invocation (`--json`, `--no-record`, …).
2. **Environment variable** (`REPOCTX_HOOK_REWRITE=off`, …).
3. **Stored `settings` row** in `.repoctx/index.db`.
4. **Built-in default**.

`repoctx config show` annotates each row with where the value came from.

## Schema

Adding a new key is non-breaking; older binaries log a warn and ignore.

| Key | Type | Values | Default | Notes |
|---|---|---|---|---|
| `hook.rewrite` | enum | `auto` \| `off` \| `force` | `auto` | Kill switch for the semantic rewrite (`off` = pure chain proxy). |
| `hook.use_rtk` | enum | `auto` \| `on` \| `off` | `auto` | Chain rtk underneath on passthrough. `auto` = on when a chainable tool is on PATH. |
| `hook.chainable` | list | comma/newline | `rtk` | Allowlist of tools repoctx may chain underneath. Only rtk is meaningful in v0.6.x. |
| `hook.script_path` | string | (read-only) | `(not installed)` | Where `repoctx init` wrote the project hook script. Computed; `config set` rejects it. |
| `gain.no_record` | bool | `true` \| `false` | `false` | Persistent `--no-record`. |
| `gain.record_query` | bool | `true` \| `false` | `false` | Persistent `--record-query`. |
| `output.default` | enum | `auto` \| `human` \| `toon` \| `json` | `auto` | Persistent output-format choice. `auto` keeps today's behavior (Human on TTY, TOON on pipe). |
| `index.nested_keys` | bool | `true` \| `false` | `false` | Index JSON/YAML/TOML keys at any depth (not just top-level). Re-index (`repoctx index --force`) after flipping. |

> Removed in 0.5.3: `hook.ref` and `hook.no_cache`. Integration content
> is embedded in the binary — there is no fetch ref or cache. Old rows in
> an existing settings table are ignored quietly.

Booleans accept `true` / `false` / `1` / `0` / `yes` / `no` on read
(case-insensitive). Writes via `config set` normalize to `true` /
`false`.

Enum values are case-insensitive on read, lowercased on write.

## Env var naming

`REPOCTX_<SECTION>_<KEY>` in screaming snake; dots become underscores:

- `REPOCTX_HOOK_REWRITE=off`
- `REPOCTX_HOOK_REF=main`
- `REPOCTX_HOOK_NO_CACHE=1`
- `REPOCTX_GAIN_NO_RECORD=1`
- `REPOCTX_GAIN_RECORD_QUERY=1`
- `REPOCTX_OUTPUT_DEFAULT=json`

The legacy `RUST_REPOCTX_NO_RECORD=1` env var keeps working as a
back-compat alias for `REPOCTX_GAIN_NO_RECORD=1`. Deprecated; prefer
the new name.

## CLI

```sh
repoctx config show              # every effective key + source
repoctx config get <key>         # one value
repoctx config set <key> <value> # validate + write
repoctx config unset <key>       # delete row, default applies again
```

`show` machine output: `{count, items: [{key, value, default, source}]}`.

Invalid `set` values fail fast with the legal set named:

```text
$ repoctx config set hook.rewrite banana
Error: hook.rewrite must be one of [auto, off, force] (got 'banana')
```

## Migration from existing flags

Every existing CLI flag that previously had no persistent home now
does. The CLI flag still works for one-shot overrides and beats the
config.

| Existing flag | Config key | One-shot still works? |
|---|---|---|
| `--no-record` | `gain.no_record` | yes |
| `--record-query` | `gain.record_query` | yes |
| `--json` / `--toon` | `output.default` | yes — beats config |
| `--verbose` / `-v` | (not migrated — one-shot only) | — |

## Storage

Lives in the `settings` table of the existing per-repo
`.repoctx/index.db`. Schema version 3 (the v3 migration creates the
table). Empty `settings` is the default state — every key absent
means "fall back to built-in default". Hand-edits to the table are
respected on next read; bad values warn on stderr and fall back to
default (the binary won't refuse to run).

## Out of scope

- Global config (`~/.config/repoctx/`). Per-repo only for now.
- TOML config files.
- Per-language config (`config index.deep.yaml` — tracked in the
  nested-keys issue under v0.6.0).
- Hot reload. Each invocation reads fresh.

## See also

- [`commands.md`](commands.md) — top-level command reference.
- [`hook.md`](hook.md) — `hook.rewrite` / `hook.use_rtk` / `hook.chainable` drive the meta-hook.
- [`gain.md`](gain.md) — `gain.no_record` / `gain.record_query` affect
  what gain analytics records.
- `wiki/decisions/2026-06-12-config-schema.md` — the binding design
  doc for the schema + precedence rules.
