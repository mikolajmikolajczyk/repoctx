# `repoctx hook` — the meta-hook

repoctx installs itself as the **single** `PreToolUse → Bash` hook in
Claude Code and chains other tools (rtk today) underneath. That makes
agent `rg`/`grep` identifier searches route through repoctx's structural
answers, while anything it doesn't rewrite still flows to the next tool —
with no race.

Most people never call `repoctx hook` directly: [`repoctx init`](init.md)
does the install. This page documents the hook itself — how the committed
script works, the runtime handler, `doctor`, and the low-level
`hook install` primitive.

## The committed script

`repoctx init` writes `.repoctx/hook.sh` (project) or
`~/.claude/repoctx-hook.sh` (global) and points `settings.json` at it by
exact path. The script is a **dumb pipe** — no `jq`, no JSON parsing:

```bash
#!/usr/bin/env bash
# repoctx-hook-version: 1
set -euo pipefail
RTK_CHAIN=1                      # 0 | 1 — chain rtk underneath on passthrough
MIN_VERSION="0.7.1"              # = the binary version that generated the script
REPOCTX="repoctx"

# repoctx missing → print install link; chain rtk if configured; never block Bash
if ! command -v "$REPOCTX" >/dev/null 2>&1; then ... fi
# cached version guard ...
exec "$REPOCTX" hook claude --rtk-chain="$RTK_CHAIN"
```

Why a committed script instead of a bare `repoctx hook claude` entry:

- **Visible + editable.** A teammate can read exactly what runs and flip
  `RTK_CHAIN`. The marker on line 2 lets `doctor` (and other tools)
  recognize it.
- **Self-documenting when repoctx is missing.** A clone without repoctx
  installed prints an install link instead of a silent no-op.
- **No jq dependency.** All JSON/rewrite/chain logic stays in the binary,
  where it's unit-tested. (rtk's script hard-requires jq and dies silently
  without it — we deliberately avoid that.)

`doctor` recomputes the expected script from the embedded template + your
config and warns if the on-disk file has drifted (tamper or staleness),
ignoring the environment-driven `RTK_CHAIN`/`MIN_VERSION`/`REPOCTX` lines.

## Runtime: `repoctx hook claude [--rtk-chain=0|1]`

Claude Code calls this with the tool-use JSON on stdin. It:

1. Tries a conservative semantic rewrite (rules below). On a hit, emits
   the `updatedInput` JSON and exits 0.
2. On a miss, if chaining is on, hands the original stdin to the first
   allowlisted tool on PATH (`hook.chainable`, rtk by default) — running
   `<tool> hook claude` and forwarding its stdout + exit verbatim.
3. Otherwise exits 1 (silent passthrough — Claude runs the original
   command).

`--rtk-chain` resolution when invoked directly (no flag):
`--rtk-chain` > `hook.use_rtk` config (`on`/`off`/`auto`, where `auto` =
an allowlisted tool is on PATH) > off. The committed script always passes
the flag explicitly.

### Rewrite rules (initial set)

| Agent pattern | Rewritten to |
|---|---|
| `rg <ident>` | `repoctx symbols <ident> --json` |
| `rg "fn <ident>"` / `"class …"` / `"struct …"` / `"function …"` | `repoctx definition <ident> --json` |
| `grep -r <ident> .` (also `-R`) | `repoctx symbols <ident> --json` |
| `grep -rn "fn <ident>" .` (and `-nr`/`-nR`/`-Rn`; class/struct/function) | `repoctx definition <ident> --json` |

**Hard passthrough**: regex (`.*`, `^`, `$`, `|`), shell metacharacters,
multiple identifiers, paths other than `.`, quoted literals
(`rg "TODO"`), and anything the conservative parser doesn't recognize.
The full decision corpus is locked behind a test suite. Disable rewrites
entirely (pure chain proxy) with `repoctx config set hook.rewrite off`.

`repoctx rewrite '<cmd>'` shows the decision for one command (exit 0 +
rewritten command, or exit 1 for passthrough) — handy for debugging.

## Why a single hook

Claude Code runs multiple `PreToolUse` hooks under the same matcher **in
parallel** and merges them **across user-global + project scopes**, with
the last-completing `updatedInput` silently winning — non-deterministic.
The only reliable design is to be the sole entry and chain everything
else in-process. That's why `init` refuses to create a configuration
where two rewriters would coexist (see [`init.md`](init.md) race table).

## Chaining other tools — `hook.chainable`

`hook.chainable` (default `["rtk"]`) is the allowlist of tools repoctx
will chain underneath on passthrough. Only rtk is meaningful today; the
key is structural so additional tools can be added without a code change.
On passthrough with chaining on, repoctx runs the first listed tool found
on PATH. If none is found it warns once (set `hook.use_rtk = off` to
silence).

## `repoctx hook doctor [-g] [--fix]`

Checks the installed hook and reports issues; `--fix` repairs them.

```sh
repoctx hook doctor          # report: drift, missing entry, foreign hooks (exit 1 if any)
repoctx hook doctor --fix    # regenerate the script + restore the settings entry
repoctx hook doctor -g       # operate on the user-global install
```

It (1) compares the on-disk script to the current template, (2) verifies
`settings.json` points at the script, and (3) lists any foreign hooks
that would race. `--fix` backs up `settings.json`, regenerates the
script, restores the entry, and clears the script's cached sentinels. Run
it after any tool reinstall that might have touched your hooks.

## `repoctx hook list` / `status` — low-level

`list` enumerates the agents repoctx can install guidance for; `status`
shows which destination files exist in a target dir. Both read embedded
manifests (offline, version-locked).

## `repoctx hook install <agent>` — low-level primitive

`init` is the supported entry point. `hook install <agent>` is the
primitive it builds on — it writes just the agent guidance files
(SKILL.md + the CLAUDE.md/AGENTS.md block), without the hook script. Use
it directly for **codex** and **opencode**, which are rules-only agents
(no PreToolUse hook to install):

```sh
repoctx hook install codex
repoctx hook install opencode
```

| Agent | File(s) | Mode |
|---|---|---|
| `claude` | `.claude/skills/repoctx/SKILL.md` | write |
| `claude` | `CLAUDE.md` block | merge-section |
| `codex` / `opencode` | `.agents/skills/repoctx/SKILL.md` | write |
| `codex` / `opencode` | `AGENTS.md` block | merge-section |

Flags: `--dir <PATH>`, `--dry-run`, `--force`. `action` values in machine
output: `created`, `updated`, `replaced_section`, `appended`,
`skipped_identical`, `skipped_marker_present`, `dry_run`.

## Distribution

Per-agent manifests + fragments are **embedded in the binary**
(`include_str!`). No network, no cache — `install` / `status` / `list`
work offline and always match your installed version. Update the content
by upgrading the binary. (Before 0.5.3 this was fetched from a GitHub
mirror; the fetcher, cache, `--ref`/`--no-cache` flags, and
`hook.ref`/`hook.no_cache` config keys were removed — the content was
always version-pinned anyway.)

## Template variables

Guidance content is templated at install time: `{REPOCTX_BIN}`,
`{REPO_NAME}`, `{REPO_ROOT}`. Plain string replacement, no escaping.

## Removing the hook

`repoctx init --uninstall [-g]` removes repoctx's entry + script (leaving
foreign/rtk entries intact). See [`init.md`](init.md#uninstall). Guidance
files are removed by hand (delete the `CLAUDE.md` block + the SKILL.md).

## Troubleshooting

- **`refusing to install: …would race`** — a foreign hook or a
  cross-scope repoctx/rtk hook exists. Follow the printed options or pass
  `--force`. See the [race table](init.md#race-resolution).
- **`hook script drifted`** (doctor) — the committed script differs from
  the current template. `repoctx hook doctor --fix` regenerates it.
- **`unknown agent: <name>`** — only `claude`, `codex`, `opencode`.
- **Chaining enabled but rtk not found** — install rtk, or
  `repoctx config set hook.use_rtk off`.

## See also

- [`init.md`](init.md) — onboarding, flags, race resolution, uninstall.
- [`config.md`](config.md) — `hook.rewrite`, `hook.use_rtk`, `hook.chainable`.
- [`commands.md`](commands.md) — full command reference.
