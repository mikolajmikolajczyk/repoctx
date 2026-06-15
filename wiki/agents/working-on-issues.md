# Working on issues

Project-specific issue workflow on top of GitHub. Issues, pull requests, and the roadmap all live on GitHub; drive everything with `gh` (the GitHub CLI). This page covers what **this project** specifically does — the label conventions and lifecycle on top of plain GitHub issues.

## Columns we use

This project uses **two** state labels by default. No `state:triage` (open-but-unlabeled issues do that job), no `state:review` (solo project, no review step). Adjust if your team is bigger.

| Label | Meaning |
|-------|---------|
| `state:in-progress` | Actively being worked. Apply **before** you start writing code. |
| `state:blocked` | Waiting on something external (decision, upstream, hardware). Pair with a `blocked:*` label that names the blocker. |
| (no state label) | Filed, scoped, not started — an open, unlabeled issue. Default for every new issue. |

Conventions:

- **Exactly one `state:*` label at a time.** When picking up an issue: `gh issue edit <N> --add-label state:in-progress`. When blocking: `gh issue edit <N> --remove-label state:in-progress --add-label state:blocked`. When finishing: `gh issue close <N>` (no need to strip `state:*` — closed issues ignore it).
- **Don't introduce `state:review`** unless a second contributor joins. Solo work doesn't need it; pretending it does just lies about the state of work.
- **`state:blocked` requires a paired `blocked:*` label** (`blocked:#N` or free-text). A naked `state:blocked` is invisible — nobody knows what's blocking.

## Branch naming — Conventional Branch

We use [conventionalbranch.org](https://conventionalbranch.org/) for any branch that isn't the default branch.

```
<type>/<short-slug>
```

Types: `feat`, `bugfix`, `hotfix`, `chore`, `docs`, `test`, `release`.

Optional issue prefix: append the issue number if it helps you find the branch later.

```
feat/multi-format-loader
feat/42-multi-format-loader          # with issue hint
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

<one paragraph: motivation, link to issue with #N>

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

## Issue → PR → closed flow

Issues, code review, and the merge all live on GitHub. Link a PR to its issue with `#N` in the commit/PR subject (or `Closes #N` in the PR body to auto-close on merge).

```sh
# 1. Start (GitHub issue)
gh issue edit <N> --add-label state:in-progress

# 2. Branch
git checkout -b feat/<N>-<slug>

# 3. Work + commit (Conventional Commits, GPG-signed)
git commit -m "feat: <subject> (#<N>)"

# 4. Push + open PR on GitHub (#N in the PR title; multi-issue: #N per commit subject)
git push -u origin feat/<N>-<slug>
gh pr create

# 5. After the PR merges into the default branch, sync your local main
git checkout main && git pull origin main

# 6. Close the GitHub issue (or let "Closes #N" in the PR body do it on merge)
gh issue close <N>
```

If a PR covers multiple issues, **don't close them until the default branch actually contains the merge**. Closing early misleads the tracker.

## Release flow

Tagged releases are rare — once per minor-version's worth of solved issues. Steps:

1. Confirm the release-engineering issue (e.g. `#1` for v0.1.0) is closed and the bench harness is green (`scripts/bench.sh`).
2. Pre-flight: `cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings`. CI must be green on `main` for ubuntu/macos/windows.
3. Bump `workspace.package.version` in the root `Cargo.toml` and the `version` literal in `flake.nix`'s `buildRustPackage` call. Move the `[Unreleased]` block in `CHANGELOG.md` into `[X.Y.Z] — YYYY-MM-DD` and add a new empty `[Unreleased]`.
4. Commit as `release: vX.Y.Z` (Conventional Commits). One commit per release.
5. Annotated, GPG-signed tag: `git tag -s vX.Y.Z -m "vX.Y.Z"`.
6. Push to GitHub: `git push origin main --tags` (the tag push triggers the release workflow).
7. Draft a GitHub release pointing at the tag — GitHub is the canonical home, so the release lives there.

**Never tag without explicit user request.** The CHANGELOG bump + flake version bump can land first as a normal patch; the tag is a separate, deliberate action.

## Decision capture inside an issue

For decisions tied to one issue, **comment on the issue**, don't open an ADR.

```sh
gh issue comment <N> -m "Decided: <choice> over <alternative> — <one-sentence reason>. Revisit if <condition>."
```

For cross-cutting decisions that don't belong to a single issue, write to `wiki/decisions/`. For app-shape decisions (architecture, contracts, constraints), write an ADR. See [`../adr/README.md`](../adr/README.md) for the three-way split.

## Session handoff

When ending a coding session mid-issue, leave a comment on the active issue:

```sh
gh issue comment <N> -m "Session pause $(date -I). Done: <X>. Next: <Y>. Blocker: <Z|none>."
```

The next session (you or an agent) reads recent comments via `gh issue view <N> --comments` and picks up without rediscovering state from the diff.

For Claude Code specifically, the same handoff also persists in auto-memory (path: `~/.claude/projects/<encoded-cwd>/memory/`). Use whichever fits — issue comments are the canonical, agent-agnostic, forge-visible surface.
