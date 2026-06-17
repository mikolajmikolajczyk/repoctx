# `repoctx init` — onboarding

`repoctx init` is the one command that wires repoctx into your coding
agent. It installs the agent guidance files and, for **Claude Code**,
adds a **SessionStart** hook that runs `repoctx prime` — so every new
session begins with a compact repo orientation digest in context and the
agent reaches for repoctx instead of blind `grep`/`cat`.

```sh
cd ~/my-project
repoctx init
```

That's it. New Claude Code sessions on this repo now start primed with a
structural map of the codebase.

## How adoption works now

repoctx orients the agent **once per session** rather than intercepting
every command. At session start Claude Code runs the SessionStart hook,
which executes `.claude/hooks/session-start.sh`; its first step is
`repoctx prime`, whose ~600-token digest (headline counts, top
subsystems, hubs, entry points, and a repoctx command cheat-sheet) is
injected into the agent's context. The agent starts knowing the repo's
shape and that `repoctx` is available.

repoctx does **not** touch `PreToolUse` and does not rewrite your
`rg`/`grep`/`find` commands. If you run your own `rtk` (or other)
PreToolUse hook, it operates completely independently of repoctx.

### The session-start script is yours to extend

`.claude/hooks/session-start.sh` is a bashrc-style script. Anything it
echoes to stdout is injected into the session context. It has a
**managed block** (regenerated on every `repoctx init`) and a **user
region** below it that is preserved across re-runs:

```bash
# >>> repoctx (managed — edits here are overwritten) >>>
repoctx prime 2>/dev/null
# <<< repoctx (managed) <<<

# --- your session-start context below (preserved across `repoctx init`) ---
echo "Reminder: ship via 'make release', never push to main."
```

Add project conventions, deploy reminders, a `git log` summary — whatever
you want the agent to see at the start of every session.

## What it writes (project scope, Claude)

| Path | What |
|---|---|
| `.claude/hooks/session-start.sh` | bashrc-style script; managed `repoctx prime` block + your own region |
| `.claude/settings.json` | a single `SessionStart` hook entry running `bash .claude/hooks/session-start.sh` |
| `.claude/skills/repoctx/SKILL.md` | the repoctx skill |
| `CLAUDE.md` | a `<!-- repoctx:start -->…<!-- repoctx:end -->` guidance block |

Commit all of these. A teammate who clones the repo gets the same
session-start priming for free (they need `repoctx` on PATH for the
digest to render; if it's missing the hook is simply a no-op).

## Flags

```
repoctx init [-g] [--agent claude] [--yes] [--force] [--dry-run]
repoctx init --uninstall [-g] [--force] [--dry-run]
```

| Flag | Effect |
|---|---|
| `-g`, `--global` | Install at user-global scope (`~/.claude/`) instead of this repo. |
| `--agent <name>` | Agent to set up: `claude` (default), `codex`, or `opencode`. |
| `--yes`, `-y` | Skip interactive prompts; take defaults / flags. |
| `--force` | Override a refused install. |
| `--dry-run` | Print the plan; write nothing. |
| `--uninstall` | Remove the SessionStart hook + strip the managed block from the script (keeps your own lines; deletes the script if it had none). Guidance files are left in place with a removal recipe. |

## Project vs global (Claude)

| | `repoctx init` | `repoctx init -g` |
|---|---|---|
| settings file | `<repo>/.claude/settings.json` | `~/.claude/settings.json` |
| SessionStart hook | yes (runs the script → `repoctx prime`) | yes (`~/.claude/hooks/session-start.sh`) |
| guidance files | yes (SKILL + CLAUDE.md) | no (no project to write into) |
| applies to | this repo | every repo you open |

Use project scope to share the setup with your team via git. Use global
scope for your own machine across all repos.

## Other agents — Codex / opencode

Codex and opencode are rules-only agents (no SessionStart protocol), so
for them `init` installs **just the guidance files**:

```sh
repoctx init --agent codex
repoctx init --agent opencode
```

These write `.agents/skills/repoctx/SKILL.md` + an `AGENTS.md` block. The
skill teaches the agent how to use `repoctx` and when to prefer it over
`rg`.

## Uninstall

```sh
repoctx init --uninstall          # project
repoctx init --uninstall -g       # global
```

Removes repoctx's SessionStart hook entry (foreign entries are left
intact) and the guidance it installed. The index (`.repoctx/index.db`)
and config are left alone — the command prints how to remove those by
hand (`rm -rf .repoctx`, delete the `CLAUDE.md` block).

## See also

- [`commands.md`](commands.md) — `repoctx prime` and the full command reference.
- [`config.md`](config.md) — per-repo settings.
- [`quickstart.md`](quickstart.md) — indexing + querying directly.
