# Forge consolidation — GitHub for everything, Radicle retired

**Date**: 2026-06-15.
**Decider**: Mikołaj Mikołajczyk.
**Tags**: process | tooling.
**Supersedes**: [`2026-06-14-github-primary-forge.md`](2026-06-14-github-primary-forge.md).

## Context

The 2026-06-14 split kept code review on GitHub but issues + roadmap on
Radicle, with dual-push (`git push origin && git push rad`) to keep the
Radicle issue repo current. In practice the dual-push + dual-tracker
overhead bought nothing for a solo project: planning happened on GitHub
PRs anyway, and keeping two issue surfaces in sync was friction with no
reader on the Radicle side.

## Decision

GitHub is the **single canonical forge** for everything — code, pull
requests, **and** issues + roadmap (`gh issue list`). Radicle is retired:
no more `rad issue`, no `git push rad`, no dual-push. Push code with
`git push origin main`. The two still-relevant Radicle issues were migrated
to GitHub before the switch:

- Radicle `3412476` → GitHub **#1** — call-site extraction, remaining languages.
- Radicle `034036f` → GitHub **#2** — agent-extensibility epic (3 children
  carried as a checklist).

The rest of the Radicle backlog was deemed not worth migrating.

## Alternatives considered

- **Keep the split (GitHub PRs + Radicle issues)** — lost: dual-tracker
  sync cost with no reader on the Radicle side.
- **Radicle-only** — lost: no inline CI status, no outside-contributor
  discoverability, no first-class review tooling.

## Consequences

- `rad`/`radboard` skills are now legacy (left on disk under
  `.agents/skills/`, removed from `.agents/skillfile` sync, dropped from the
  `AGENTS.md` pointer table).
- Session-start hook + session-handoff flow use `gh issue …`.
- `settings.local.json` allowlists `gh issue *` instead of `rad issue *`.
- Issue references repo-wide are GitHub `#N`, not Radicle `hex7`.
- Historical CHANGELOG entries that mention Radicle are kept as-is — they
  record what was true at the time.

## Trigger to revisit

Only if GitHub becomes unacceptable (e.g. account/hosting loss) — then
re-evaluate a sovereign forge. Not expected.
