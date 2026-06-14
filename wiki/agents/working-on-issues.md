# Working on issues

Project-specific addendum to the [radboard skill](../../.agents/skills/radboard/SKILL.md). Radboard covers the universal lifecycle (open → in-progress → review → solved + hex7 patch linking). This page covers what **this project** specifically does on top.

## Columns we use

The radboard skill says "pick whatever workflow you want". This project uses **two** state labels by default. No `state:triage` (the built-in Open column does that job), no `state:review` (solo project, no review step). Adjust if your team is bigger.

| Label | Meaning |
|-------|---------|
| `state:in-progress` | Actively being worked. Apply **before** you start writing code. |
| `state:blocked` | Waiting on something external (decision, upstream, hardware). Pair with a `blocked:*` label that names the blocker. |
| (no state label) | Filed, scoped, not started — sits in the built-in **Open** column. Default for every new issue. |

Conventions:

- **Exactly one `state:*` label at a time.** When picking up an issue: `-a state:in-progress`. When blocking: `-d state:in-progress -a state:blocked`. When finishing: `rad issue state --solved <ID>` (no need to strip `state:*` — solved issues ignore it).
- **Don't introduce `state:review`** unless a second contributor joins. Solo work doesn't need it; pretending it does just makes the board lie.
- **`state:blocked` requires a paired `blocked:*` label** (hex7 or free-text). A naked `state:blocked` is invisible — nobody knows what's blocking.

## Branch naming — Conventional Branch

We use [conventionalbranch.org](https://conventionalbranch.org/) for any branch that isn't the default branch.

```
<type>/<short-slug>
```

Types: `feat`, `bugfix`, `hotfix`, `chore`, `docs`, `test`, `release`.

Optional issue prefix: append the 7-char hex if it helps you find the branch later.

```
feat/multi-format-loader
feat/3b73e5d-multi-format-loader     # with issue hint
chore/eslint-boundaries
docs/adr-0002-layering
```

Why a convention at all on a solo project: future-me, AI agents, and `git branch --list 'feat/*'` queries all want predictability.

Push branch + open a PR on GitHub:

```sh
git push -u origin feat/<slug>
gh pr create
```

Conventional Branch is **not** Conventional Commits — commit messages still follow Conventional Commits separately (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `release:`).

## PR description template

Not a hard requirement, but matches what the project expects. Put this in the PR body (`gh pr create --body …` or the GitHub editor):

```markdown
## Why

<one paragraph: motivation, link to issue with hex7>

## What

<bulleted summary of the changes>

## Acceptance

- [ ] criterion 1 from the issue
- [ ] criterion 2
- [ ] criterion 3

## Notes

<anything reviewers / future-you should know>
```

Checked boxes in the PR body let future-you see at a glance what landed vs what slipped.

## Issue → PR → solved flow

Issues stay on Radicle; code review is a GitHub PR. The two are linked by the hex7 in the commit/PR subject.

```sh
# 1. Start (Radicle issue)
rad issue label <hex7> -a state:in-progress

# 2. Branch
git checkout -b feat/<hex7>-<slug>

# 3. Work + commit (Conventional Commits, GPG-signed)
git commit -m "feat: <subject> (<hex7>)"

# 4. Push + open PR on GitHub (hex7 in the PR title; multi-issue: hex7 per commit subject)
git push -u origin feat/<hex7>-<slug>
gh pr create

# 5. After the PR merges into the default branch, sync the Radicle mirror
git checkout main && git pull origin main && git push rad main

# 6. Mark the Radicle issue solved
rad issue state --solved <hex7>
```

If a PR covers multiple issues, **don't `--solved` them until the default branch actually contains the merge**. Solving early misleads the board.

## Release flow

Tagged releases are rare — once per minor-version's worth of solved issues. Steps:

1. Confirm the release-engineering issue (e.g. `bc9da7c` for v0.1.0) is solved and the bench harness is green (`scripts/bench.sh`).
2. Pre-flight: `cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings`. CI must be green on `main` for ubuntu/macos/windows.
3. Bump `workspace.package.version` in the root `Cargo.toml` and the `version` literal in `flake.nix`'s `buildRustPackage` call. Move the `[Unreleased]` block in `CHANGELOG.md` into `[X.Y.Z] — YYYY-MM-DD` and add a new empty `[Unreleased]`.
4. Commit as `release: vX.Y.Z` (Conventional Commits). One commit per release.
5. Annotated, GPG-signed tag: `git tag -s vX.Y.Z -m "vX.Y.Z"`.
6. Push to GitHub (primary) then mirror to Radicle: `git push origin main --tags`, then `git push rad main && git push rad vX.Y.Z`.
7. Draft a GitHub release pointing at the tag — GitHub is the canonical code home, so the release lives there.

**Never tag without explicit user request.** The CHANGELOG bump + flake version bump can land first as a normal patch; the tag is a separate, deliberate action.

## Decision capture inside an issue

For decisions tied to one issue, **comment on the issue**, don't open an ADR.

```sh
rad issue comment <hex7> -m "Decided: <choice> over <alternative> — <one-sentence reason>. Revisit if <condition>."
```

For cross-cutting decisions that don't belong to a single issue, write to `wiki/decisions/`. For app-shape decisions (architecture, contracts, constraints), write an ADR. See [`../adr/README.md`](../adr/README.md) for the three-way split.

## Session handoff

When ending a coding session mid-issue, leave a comment on the active issue:

```sh
rad issue comment <hex7> -m "Session pause $(date -I). Done: <X>. Next: <Y>. Blocker: <Z|none>."
```

The next session (you or an agent) reads recent comments via `rad issue show <hex7>` and picks up without rediscovering state from the diff.

For Claude Code specifically, the same handoff also persists in auto-memory (path: `~/.claude/projects/<encoded-cwd>/memory/`). Use whichever fits — issue comments are the canonical, agent-agnostic, forge-visible surface.
