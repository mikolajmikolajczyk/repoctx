# `repoctx hook` — per-agent install

`repoctx hook` drops the `repoctx` skill / guidance into a target repo so AI coding agents (Claude Code, Codex, opencode) auto-load it. Three subcommands: `list`, `status`, `install`. No `uninstall` — `install` prints removal instructions on success.

## What it does, by agent

| Agent | File(s) written | Mode |
|---|---|---|
| `claude` | `.claude/skills/repoctx/SKILL.md` | write |
| `claude` | `CLAUDE.md` (merge-section block) | merge-section |
| `codex` | `.agents/skills/repoctx/SKILL.md` | write |
| `codex` | `AGENTS.md` (merge-section block) | merge-section |
| `opencode` | `.agents/skills/repoctx/SKILL.md` | write |
| `opencode` | `AGENTS.md` (merge-section block) | merge-section |

`SKILL.md` is shared content — one source of truth at `integrations/shared/SKILL.md`, three different destinations. Codex + opencode also share the `AGENTS.md` fragment, so installing both into one repo is a no-op on the second pass.

## Quick start

```sh
cd ~/my-project
repoctx hook install claude
```

Output (machine):

```json
{
  "agent": "claude",
  "dir": "/home/me/my-project",
  "written": [
    {"path": ".../.claude/skills/repoctx/SKILL.md", "mode": "write",
     "bytes": 4719, "action": "created"},
    {"path": ".../CLAUDE.md", "mode": "merge-section",
     "bytes": 986, "action": "created"}
  ],
  "removal": "Installed claude. To remove: …"
}
```

The skill is now active in any Claude Code session opened on this repo.

## Modes

The installer reads each manifest's `mode` field per file:

- **`write`** — create the destination, or overwrite when the existing content already matches (no-op) or the caller passes `--force` (Updated). A pre-existing differing file without `--force` errors out so a hand-edited skill is never clobbered silently.
- **`append`** — append the source to the destination. Idempotent via a marker substring (`start_marker`): if the marker is already present, the append is skipped.
- **`merge-section`** — wrap the source in `start_marker` / `end_marker` and either replace the existing block between those markers or append a fresh one. Re-runs with unchanged content land at `skipped_identical`.

## Subcommands

### `repoctx hook list`

Enumerate the three supported agents and their descriptions.

```sh
repoctx hook list
```

```text
ref: v0.5.1
claude    Claude Code skill at .claude/skills/repoctx/SKILL.md + a CLAUDE.md guidance block.
codex     Codex CLI guidance via merge-section block in AGENTS.md.
opencode  opencode CLI guidance via merge-section block in AGENTS.md.
```

Descriptions come from the per-agent manifest. If the fetch fails (no network, bad `--ref`), the row shows just the name + a hint to retry.

### `repoctx hook status`

Probe a target directory for already-installed files.

```sh
repoctx hook status
```

```text
ref: v0.5.1
dir: /home/me/my-project

claude:
  ✓  .claude/skills/repoctx/SKILL.md  [write]
  ·  CLAUDE.md                        [merge-section]

codex:
  ·  AGENTS.md                        [merge-section]

opencode:
  ·  AGENTS.md                        [merge-section]
```

`✓` = present on disk, `·` = not yet installed. `status` doesn't open the files — it only checks for existence.

### `repoctx hook install <agent>`

Install one agent into the current repo (or `--dir PATH`).

| Flag | Effect |
|---|---|
| `--dir <PATH>` | Target directory. Defaults to the repo root. |
| `--dry-run` | Plan the install, print the actions, write nothing. |
| `--force` | Overwrite write-mode files whose current content differs from upstream. |
| `--ref <git-ref>` | Pull manifests + files from a specific git ref. Default `v<binary version>`. Use `--ref main` to pick up unreleased work. |
| `--no-cache` | Bypass the on-disk cache; always fetch from network. |

`action` values you'll see in machine output:

- `created` — destination did not exist.
- `updated` — `write` mode + `--force` overwrote a differing file.
- `replaced_section` — `merge-section` replaced an existing marker block.
- `appended` — `append` or `merge-section` added new content.
- `skipped_identical` — destination already contained the expected bytes.
- `skipped_marker_present` — `append` mode found its marker already.
- `dry_run` — `--dry-run` was set; nothing written.

## Transparent rewrite (Claude only)

When `repoctx hook install claude` runs, it does two things beyond
writing the skill + CLAUDE.md fragment:

1. **Takes ownership** of `.claude/settings.json`'s
   `PreToolUse → Bash` matcher. Any pre-existing hook entries
   (typically [`rtk`](https://github.com/rtk-ai/rtk) or other agent
   tooling) are saved into `hook.chain_commands` config and removed
   from `settings.json`. A single new entry pointing at
   `repoctx hook claude` is inserted.
2. **Chains through the displaced hooks at runtime.** When Claude
   Code fires the hook, `repoctx hook claude`:
   - Tries our semantic rewrites first (`rg <ident>` →
     `repoctx symbols <ident>`, `grep -rn "fn <name>" .` →
     `repoctx definition <name>`, etc.).
   - On miss, executes each saved chain command in order with the
     same stdin payload. The first one that returns a rewrite wins.
   - On all-miss, exits 1 (silent passthrough — Claude Code runs the
     original command).

### Why ownership-takeover

Claude Code runs multiple PreToolUse hooks **in parallel** when they
share the same matcher. The last-to-complete `updatedInput` silently
overwrites everyone else's — non-deterministic. The only reliable
way to coexist with rtk (and other rewrite hooks) is to be the sole
entry under the matcher and explicitly chain through everything
else.

### Rewrite rules (initial set)

| Agent pattern | Rewritten to |
|---|---|
| `rg <ident>` | `repoctx symbols <ident> --json` |
| `rg "fn <ident>"` / `"class <ident>"` / `"struct <ident>"` / `"function <ident>"` | `repoctx definition <ident> --json` |
| `grep -r <ident> .` (also `-R`) | `repoctx symbols <ident> --json` |
| `grep -rn "fn <ident>" .` (and `-nr`/`-nR`/`-Rn`; class/struct/function) | `repoctx definition <ident> --json` |

**Hard passthrough**: regex (`.*`, `^`, `$`, `|`, etc.), shell
metacharacters (`|`, `&`, `;`, `$`, backticks, `>`), multiple
identifiers, paths other than `.`, quoted single literals
(`rg "TODO"` — user wanted a string match, not a symbol search),
and anything else the conservative parser doesn't recognize.

Disable the rewrite layer entirely while keeping the chain: `repoctx
config set hook.rewrite off`. The hook becomes a pure proxy in front
of whatever rtk-style chain you have. Default is `auto`. There's
also `force` (relax the parser) — debug-only, not recommended.

### `repoctx hook doctor`

Run this anytime you suspect another installer (rtk reinstall, manual
`.claude/settings.json` edit) has added a sibling entry under the
Bash matcher. It re-runs the takeover step idempotently:

```sh
repoctx hook doctor             # take ownership, save any new chain
repoctx hook doctor --dry-run   # preview what would change
```

Recommendation: run `repoctx hook doctor` after any other
PreToolUse-touching install. Optional but worth adding to a shell
alias if you frequently update tooling that wires hooks.

### Removing the rewrite hook

Two steps:

1. Restore the original `.claude/settings.json` entry. The install
   recipe printed at install time names the commands we saved (or
   read them from `repoctx config get hook.chain_commands`).
2. Run `repoctx config unset hook.chain_commands` to drop our
   record.

Don't forget to remove the `repoctx hook claude` entry from
`.claude/settings.json` itself if it's still there after restoring
the original.

### Telemetry

Rewrites don't write a `usage` row today (the rewritten command
already gets its own row when it runs). If you want to see hook
activity, check stderr in your Claude Code logs — each rewrite emits
`repoctx rewrote (<rule>): <orig> → <new>` as the
`permissionDecisionReason`.

## Distribution

Per-agent files are NOT baked into the binary. Each `install` / `status` / `list` invocation:

1. Looks up `<XDG_CACHE_HOME>/repoctx/integrations/<ref>/<agent>/manifest.toml` (cache hit serves it).
2. On cache miss, GETs `https://raw.githubusercontent.com/mikolajmikolajczyk/repoctx/<ref>/integrations/<agent>/manifest.toml`.
3. Same dance for each file the manifest references.

Cache layout: `~/.cache/repoctx/integrations/v0.5.1/claude/SKILL.md` (Linux), `~/Library/Caches/dev.repoctx.repoctx/integrations/v0.5.1/...` (macOS), `%LOCALAPPDATA%\repoctx\repoctx\cache\integrations\v0.5.1\...` (Windows). `REPOCTX_INTEGRATIONS_CACHE_DIR` overrides the root. Pre-populating the cache by hand is a supported offline path — the installer doesn't distinguish between "cached because we fetched it" and "cached because you wrote it there".

## Template variables

Per-file source content is templated at install time:

| Variable | Resolved to |
|---|---|
| `{REPOCTX_BIN}` | `std::env::current_exe()` of the binary running the install. |
| `{REPO_NAME}` | Final path component of `--dir` (or the repo root). |
| `{REPO_ROOT}` | Absolute path of `--dir` (or the repo root). |

Variables not in this set are left untouched. Plain string replacement — no escaping, no expressions.

## Removing what `install` wrote

There's no `repoctx hook uninstall`. Every `install` prints a per-file removal recipe at the end, e.g.:

```text
Installed claude. To remove:
  - rm .claude/skills/repoctx/SKILL.md
  - in CLAUDE.md, delete the block between `<!-- repoctx:start -->` and `<!-- repoctx:end -->` (inclusive)
```

Follow it by hand. For `merge-section` files, deleting the entire block (markers included) is the canonical removal — nothing else in `CLAUDE.md` / `AGENTS.md` is touched.

## Troubleshooting

- **`fetch failed: GET … status code 404`** — usually means the `--ref` doesn't have content yet. Try `--ref main` to pick up the latest in-development manifests.
- **`refusing to write … destination exists with different content`** — you've edited the file locally. Re-run with `--force` to overwrite, or merge upstream changes by hand.
- **`unknown agent: <name>`** — only `claude`, `codex`, and `opencode` are supported in v0.1.x. Open an issue for additional agents.
- **Cache stale after a release** — `--no-cache` forces a refetch. The cache directory is safe to delete (`rm -rf ~/.cache/repoctx/integrations/`).

## Coexistence with user-global tools — known limitation

repoctx is deliberately **project-scoped** — `repoctx hook install
claude` only writes `<project>/.claude/settings.json`. It never
touches `~/.claude/settings.json`.

Claude Code's runtime, however, **merges hooks across all scopes**.
A `PreToolUse → Bash` entry in `~/.claude/settings.json` (e.g. from
`rtk init -g`) fires in parallel with our project-local entry. The
last-completing `updatedInput` wins — non-deterministic.

**Detection**: `repoctx hook install claude` and `repoctx hook
doctor` both scan `~/.claude/settings.json` and emit a stderr
warning listing every conflicting command they find. We don't
silently take ownership of user-global config; that's the user's
domain.

**Workarounds** (in order of preference):

1. **Install the conflicting tool per-project, not user-global.**
   Run rtk (or whatever) without `-g` / `--global`. Our takeover
   then sees its entry in `<project>/.claude/settings.json` and
   chains it cleanly.
2. **Disable the user-global entry by hand.** Edit
   `~/.claude/settings.json` and remove or comment out the Bash
   matcher entry.
3. **Live with the race.** Acceptable if the conflicting tool's
   rewrite is non-destructive (rtk's compression is). Run
   `repoctx hook doctor` after any reinstall to keep our entry
   present.

This is a Claude Code design limitation (no `exclusive` / `override`
flag for hook scopes), not a repoctx bug. Feedback channel: file
with Anthropic via `/feedback` if it bites you.

## Coexistence with other hook installers

If you use [`rtk`](https://github.com/rtk-ai/rtk) (or another
PreToolUse-installing tool), follow this order:

1. Install rtk normally.
2. Install repoctx: `repoctx hook install claude`. It detects rtk's
   entry, displaces it into `hook.chain_commands`, and inserts our
   own.
3. **If you ever reinstall rtk**, it may overwrite our entry. Re-run
   `repoctx hook doctor` to re-take ownership.

repoctx's rewrite rules are conservative — only `rg <ident>` family
patterns. Everything else (git/docker/test/lint/etc) falls through to
rtk's compressing rewrites unchanged.

## See also

- [`commands.md`](commands.md) — full `repoctx` command reference.
- [`output-formats.md`](output-formats.md) — how the installed skill tells the agent to format output.
