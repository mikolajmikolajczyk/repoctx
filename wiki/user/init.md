# `repoctx init` — onboarding

`repoctx init` is the one command that wires repoctx into Claude Code. It
installs a small committed hook script, points Claude Code at it, and
drops the agent guidance files — so an agent in this repo navigates code
through repoctx instead of grepping.

```sh
cd ~/my-project
repoctx init
```

That's it. New Claude Code sessions on this repo now route `rg`/`grep`
identifier searches through repoctx automatically.

## What it writes (project scope)

| Path | What |
|---|---|
| `.repoctx/hook.sh` | the committed dumb-pipe hook script (executable) |
| `.claude/settings.json` | a single `PreToolUse → Bash` entry pointing at the script |
| `.gitattributes` | `*.sh text eol=lf` (keeps the script LF + executable across platforms) |
| `.claude/skills/repoctx/SKILL.md` | the repoctx skill |
| `CLAUDE.md` | a `<!-- repoctx:start -->…<!-- repoctx:end -->` guidance block |

Commit all of these. A teammate who clones the repo gets the hook for
free; if they don't have `repoctx` installed, the committed script prints
an install link instead of failing silently.

The hook script is a thin bootstrap — it just execs `repoctx hook claude`.
All the rewrite/JSON/chain logic lives in the binary. You can read and
edit `.repoctx/hook.sh` (e.g. flip `RTK_CHAIN`); `repoctx hook doctor`
will tell you if it has drifted from what the current binary expects.

## Flags

```
repoctx init [-g] [--agent claude] [--rtk auto|on|off] [--yes] [--force] [--dry-run]
repoctx init --uninstall [-g] [--restore-backup] [--force] [--dry-run]
```

| Flag | Effect |
|---|---|
| `-g`, `--global` | Install at user-global scope (`~/.claude/`) instead of this repo. |
| `--agent <name>` | Agent to set up. Only `claude` today (codex/opencode use `repoctx hook install`). |
| `--rtk auto\|on\|off` | Chain [rtk](https://github.com/rtk-ai/rtk) underneath on passthrough. `auto` = on when rtk is on PATH. |
| `--yes`, `-y` | Skip interactive prompts; take defaults / flags. |
| `--force` | Override a refused install (race) or remove a drifted script. |
| `--dry-run` | Print the plan; write nothing. |
| `--uninstall` | Remove repoctx's hook (inverse of install). |
| `--restore-backup` | With `--uninstall -g`: restore the most recent settings backup. |

## Project vs global

| | `repoctx init` | `repoctx init -g` |
|---|---|---|
| settings file | `<repo>/.claude/settings.json` | `~/.claude/settings.json` |
| hook script | `<repo>/.repoctx/hook.sh` | `~/.claude/repoctx-hook.sh` |
| guidance files | yes (SKILL + CLAUDE.md) | no (no project to write into) |
| applies to | this repo | every repo you open |

Use project scope to share the setup with your team via git. Use global
scope for your own machine across all repos.

## rtk coexistence

repoctx becomes the single hook and chains rtk underneath, so you get
repoctx's structural rewrites *and* rtk's output compression with no
race. On `init -g` over a pre-existing global rtk hook, repoctx backs up
your `settings.json`, takes over, and turns rtk chaining on automatically
(`--rtk off` opts out, with a warning).

## Race resolution

Claude Code merges `PreToolUse` hooks across user-global and project
scopes and runs same-matcher hooks in parallel with a non-deterministic
result. So `init` refuses to create a configuration that would race, and
tells you how to fix it:

| You ran | and found | init does | fix |
|---|---|---|---|
| `init` (project) | a **foreign** hook (not repoctx/rtk) anywhere | refuses | remove/disable it, or `--force` |
| `init` (project) | a **global rtk** hook | refuses | `repoctx init -g` (recommended), or uninstall global rtk, or `--force` |
| `init` (project) | a **global repoctx** hook | **installs guidance only** (skill + CLAUDE.md), skips the redundant project hook | nothing — the global hook already fires here; `--force` to add a project hook anyway |
| `init -g` | a **project repoctx** hook | refuses (double-fire) | remove the project install first, or `--force` |
| `init` again, same scope | repoctx already installed | re-installs idempotently | — |

A global repoctx hook already runs for every project, so a project-local
hook would only double-fire. `init` installs the guidance files (which
never race) and leaves the hook to the global install. `--force` accepts
the double-fire and installs a project hook anyway.

## Uninstall

```sh
repoctx init --uninstall          # project
repoctx init --uninstall -g       # global
repoctx init --uninstall -g --restore-backup   # also restore the pre-takeover settings
```

Removes repoctx's own Bash hook entry (foreign/rtk entries are left
intact) and deletes the hook script when it's verifiably ours. The index
(`.repoctx/index.db`), config, and guidance files are left alone — the
command prints how to remove those by hand (`rm -rf .repoctx`, delete the
`CLAUDE.md` block).

## Keeping it healthy

`repoctx hook doctor [-g]` checks the installed hook for drift/tamper and
scope conflicts; `repoctx hook doctor --fix` regenerates the script and
restores the settings entry. See [`hook.md`](hook.md) for the full hook
reference.

## See also

- [`hook.md`](hook.md) — the meta-hook in depth: script anatomy, doctor, chainable tools.
- [`config.md`](config.md) — `hook.use_rtk`, `hook.chainable`, `hook.rewrite`.
- [`quickstart.md`](quickstart.md) — indexing + querying without the hook.
