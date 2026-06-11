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
ref: v0.2.0
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
ref: v0.2.0
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

## Distribution

Per-agent files are NOT baked into the binary. Each `install` / `status` / `list` invocation:

1. Looks up `<XDG_CACHE_HOME>/repoctx/integrations/<ref>/<agent>/manifest.toml` (cache hit serves it).
2. On cache miss, GETs `https://raw.githubusercontent.com/mikolajmikolajczyk/repoctx/<ref>/integrations/<agent>/manifest.toml`.
3. Same dance for each file the manifest references.

Cache layout: `~/.cache/repoctx/integrations/v0.2.0/claude/SKILL.md` (Linux), `~/Library/Caches/dev.repoctx.repoctx/integrations/v0.2.0/...` (macOS), `%LOCALAPPDATA%\repoctx\repoctx\cache\integrations\v0.2.0\...` (Windows). `REPOCTX_INTEGRATIONS_CACHE_DIR` overrides the root. Pre-populating the cache by hand is a supported offline path — the installer doesn't distinguish between "cached because we fetched it" and "cached because you wrote it there".

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

## See also

- [`commands.md`](commands.md) — full `repoctx` command reference.
- [`output-formats.md`](output-formats.md) — how the installed skill tells the agent to format output.
