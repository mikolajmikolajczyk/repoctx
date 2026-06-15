# Forge split — GitHub for code, Radicle for issues

**Date**: 2026-06-14.
**Status**: superseded by [`2026-06-15-github-only-forge.md`](2026-06-15-github-only-forge.md) (full switch to GitHub; Radicle retired entirely).

## What

Code review moves to **GitHub pull requests**. **Radicle** keeps internal
issue tracking + roadmap. Previously Radicle was the canonical forge for
everything (code patches via `git push rad HEAD:refs/patches`) and GitHub
was a CI-only read-only mirror.

## New flow

- **Code review**: branch + `gh pr create` on
  <https://github.com/mikolajmikolajczyk/repoctx>. The `rad patch` /
  `refs/patches` workflow is retired.
- **Issues + roadmap**: stay on Radicle (`rad issue …`). Still the source
  of truth for milestones/backlog. Labels per the radboard skill.
- **Push targets**: dual-push code — `git push origin main` (primary) **and**
  `git push rad main` — so the Radicle repo that hosts issues stays current.
- **Releases**: GitHub is the canonical code home; releases live there
  (`.github/workflows/release.yml`), tag pushed to origin first, then mirrored.

## Why

GitHub PRs give first-class review tooling, CI status checks inline, and
discoverability for outside contributors — Radicle patches reached none of
those. Radicle still earns its keep as a sovereign, forge-visible,
agent-agnostic issue tracker, which is where the project's planning lives.

## Consequences

- This reverses the "canonical forge = Radicle" line in `AGENTS.md`,
  `README.md`, `wiki/agents/{dev-setup,working-on-issues,commands}.md`.
- `git push rad HEAD:refs/patches` no longer appears in any flow.
- The radicle skill is now scoped to *issues* (its `rad` cheat-sheet still
  applies); code-review steps point at `gh`.
- Not promoted to an ADR: it's a workflow decision for a solo maintainer,
  not an app-shape contract. Revisit (→ ADR) if collaborators join.
