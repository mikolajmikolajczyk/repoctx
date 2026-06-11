# AGENTS.md — repoctx

Repo-specific notes for coding agents (Claude Code, Codex, opencode, Cursor, Aider, Copilot, …). Generic software-engineering advice is out of scope. End users wire `repoctx` into their own agent with `repoctx hook install <agent>` — see [`wiki/user/hook.md`](wiki/user/hook.md).

> **CLAUDE.md** at repo root is `@AGENTS.md` plus Claude-only overrides. Other agents read this file directly.


## What this is

`repoctx` is an AI-oriented repository intelligence CLI. It provides fast semantic code navigation over a local repository using Tree-sitter as the primary indexing backend, SQLite as the source-of-truth metadata store, and optional LSP backends for richer code intelligence. All machine-consumable commands support JSON output. Designed for incremental, mtime-based cache invalidation so re-indexing stays cheap.

## Where things live

| Need | Path | When to load |
|------|------|--------------|
| **Source of truth for roadmap, milestones, backlog** | Radicle issues — `rad issue list --all` | Always. **Don't read roadmaps from markdown.** |
| Current repo shape, data flow, file map | [`wiki/agents/architecture.md`](wiki/agents/architecture.md) | When making structural changes or unfamiliar with module layout |
| Coding conventions, file naming, commit style, comment policy | [`wiki/agents/conventions.md`](wiki/agents/conventions.md) | Before writing or modifying code |
| Feature status (what works, what's in flight, what's broken) | [`wiki/agents/status.md`](wiki/agents/status.md) | When user asks "does X work?" or you're picking up work |
| Common dev commands (build, test, run, typecheck, lint) | [`wiki/agents/commands.md`](wiki/agents/commands.md) | When running build/test/dev loops |
| Tooling (devShell, direnv, pre-commit, static analysis) | [`wiki/agents/dev-setup.md`](wiki/agents/dev-setup.md) | When fixing tooling, adding hooks, or onboarding |
| Working on issues (state columns, branch naming, patch flow, session handoff) | [`wiki/agents/working-on-issues.md`](wiki/agents/working-on-issues.md) | Before picking up a Radicle issue |
| Where to capture decisions (ADR vs decision log vs comment) | [`wiki/adr/README.md`](wiki/adr/README.md) | When making a non-trivial decision |
| Project glossary / domain terminology | [`wiki/agents/glossary.md`](wiki/agents/glossary.md) | When you hit an unfamiliar term |
| Things deliberately deferred — do NOT implement unprompted | [`wiki/agents/deferred.md`](wiki/agents/deferred.md) | Before adding features that "seem missing" |
| Architecture Decision Records | [`wiki/adr/`](wiki/adr/) | When touching subsystems an ADR covers |
| Cross-cutting decisions (not big enough for ADR) | [`wiki/decisions/`](wiki/decisions/) | Before reversing a prior call |
| Radicle skill (`rad` CLI usage) | [`.agents/skills/radicle/SKILL.md`](.agents/skills/radicle/SKILL.md) | Auto-loaded by radicle skill trigger; also when driving `rad` manually |
| Radboard skill (label conventions for kanban) | [`.agents/skills/radboard/SKILL.md`](.agents/skills/radboard/SKILL.md) | Before adding labels to issues/patches |

> **Skills location.** Canonical: `.agents/skills/<name>/` (agent-agnostic). `.claude/skills/` are symlinks created by `scripts/skills-bootstrap.sh` so Claude Code can auto-trigger them. To refresh skills: re-run `scripts/skills-bootstrap.sh`. To add/remove skills: edit `.agents/skillfile`, then re-run.

## Load-on-demand rule

Don't read every wiki file at session start. Pick the file matching the task — they are sized to be loaded individually. The table above tells you *when* to load *what*.

## Session handoff

When ending a session mid-issue, drop a one-line comment on the active issue describing what's done, what's next, and any blocker:

```sh
rad issue comment <hex7> -m "Session pause $(date -I). Done: <X>. Next: <Y>. Blocker: <Z|none>."
```

When starting a session, read recent comments on the most-recently-touched in-progress issue (`rad issue list --label state:in-progress`, then `rad issue show <hex7>`) before doing anything else. Forge-visible, agent-agnostic.

Details: [`wiki/agents/working-on-issues.md`](wiki/agents/working-on-issues.md).

## Working on issues / patches

This repo uses **Radicle** as its canonical forge (any GitHub/GitLab mirror is CI-only). Read [`.agents/skills/radicle/SKILL.md`](.agents/skills/radicle/SKILL.md) before driving `rad`. Issues follow [`.agents/skills/radboard/SKILL.md`](.agents/skills/radboard/SKILL.md) label conventions (`state:*`, `priority:*`, `milestone:*`, `epic`, `parent:<hex7>`, `blocked:*`).

## Quick dev loop

```sh
nix develop                    # enter pinned Rust devShell (or trust direnv `use flake`)
cargo build
cargo run -- --help
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Full list in [`wiki/agents/commands.md`](wiki/agents/commands.md).

## Hard rules (don't violate)

- **Never commit without explicit user request.** Even mid-task, after accepting a plan, stop and ask. Acceptance of plan ≠ acceptance of commit.
- **Don't add features, refactor, or introduce abstractions beyond what the task requires.** Bug fix = bug fix, not surrounding cleanup.
- **Don't pre-empt later milestones.** If something carries a future `milestone:*` Radicle label, don't half-implement it while working on the current one.
- **All project docs live under `wiki/`.** If you find a `docs/` folder, move its contents to `wiki/` and delete the old folder.

## Code ownership

Mikołaj Mikołajczyk — solo maintainer and sole deciding authority.
