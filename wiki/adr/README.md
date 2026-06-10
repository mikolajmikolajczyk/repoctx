# When to write an ADR

ADRs are append-only records of decisions that are **expensive to reverse**, **constrain future choices**, or **need explaining a year from now**. They are not a journal of every change. The bar for adding one is deliberately high so the index stays scannable.

## The three-way split

This project captures decisions in three places. Pick the right one:

| Surface | Use when | Lifetime |
|---------|----------|----------|
| **ADR** (this folder) | Decision constrains app shape, public contracts, layering, error/test/runtime semantics. Hard to reverse. Affects every future contributor. | Project-lifetime, append-only |
| **Decision log** ([`../decisions/`](../decisions/)) | Cross-cutting tool / library / process choice not tied to a single issue. Reversible in days. Examples: which library fills a small role, whether generated artifacts ship in-tree or get rebuilt in CI, AI-agent permissions. | Until superseded; lightweight |
| **Issue comment / commit message** | Decision tied to one issue or one commit. Examples: "for `c5aaf5a` we chose encoding X over Y because of constraint Z." | Bound to that issue / commit |

If a decision spans more than the immediate work but isn't an architectural promise, it belongs in `../decisions/` — not as a fourth ADR. ADR overhead (full template, ceremony, numbered slot, append-only discipline) is wasted on a library swap.

## Write an ADR when the decision

- **Constrains the shape of the app or public contracts** — once shipped, downstream code depends on it.
- **Is hard to reverse** — undoing it requires a migration, not a refactor.
- **Affects cross-cutting concerns** — touches multiple layers / modules / milestones.
- **Was contested or non-obvious** — there were real alternatives and someone, future-you included, will want the rationale.
- **Has stakeholder implications** — onboarding, distribution, licensing, hosting.

## Skip the ADR when the decision

- Is a **tool choice** that can be swapped in a day (formatter, linter, package manager, devShell tech).
- Is **DX convenience** with no behavioral effect (editor config, direnv, shell aliases).
- Is a **library swap** in a single layer with no contract change.
- Belongs in a **PR description, commit message, or code comment** because it only affects that change.
- Is **a status update or roadmap item** — those live in Radicle issues, not ADRs.

## Concrete examples

### ADR-worthy

| Topic | Why |
|-------|-----|
| Layering rules + dependency direction | Constrains every future import; load-bearing |
| Plugin / extension host model | Locks the transport contract |
| Error boundary strategy | Cross-cutting; defines failure contract per layer |
| Testing strategy (contract harnesses, integration split) | Shapes what authors ship alongside code |
| Schema / data-format version that's not backward-compatible | External contract; hard cut, no shim |
| Public interface for a plugin / extension point | Downstream authors depend on it |
| Monorepo split timing | Cross-cutting; affects build, packaging, publishing |
| License choice | Stakeholder + distribution implications |

### NOT ADR-worthy

| Topic | Where it lives instead |
|-------|------------------------|
| Nix flake devShell (or mise/asdf/rustup choice) | [`../agents/dev-setup.md`](../agents/dev-setup.md) |
| Direnv `.envrc` | [`../agents/dev-setup.md`](../agents/dev-setup.md) |
| Pre-commit framework + hook list | [`../agents/dev-setup.md`](../agents/dev-setup.md) and `.pre-commit-config.yaml` |
| Formatter / linter choice | Config file + dev-setup page |
| Editor recommendations | dev-setup page |
| Small-role library swap (no contract change) | PR description + code comment |
| Bumping a pinned dependency | Commit message |
| Adding a new entry to a built-in pack | Commit message + radicle issue |

### Edge cases — write an ADR if the answer is "yes"

- **Tool choice with lock-in:** "Build *requires* Nix" → ADR. "Nix is primary, npm works as fallback" → no ADR.
- **Library swap that changes a public interface:** if downstream code notices the change → ADR. If purely internal → no ADR.
- **Process / workflow decision** (e.g. "Patches go through Radicle, GitHub mirror is CI-only") → ADR if it's a durable contract with collaborators; skip if it's a personal preference.

## Format

Use existing ADRs in this directory as the template. Minimum sections:

- **Status** — Proposed / Accepted / Superseded by ADR-NNNN
- **Date** — ISO date of acceptance
- **Deciders** — names
- **Tags** — short labels for searchability
- **Context** — what's the situation
- **Decision drivers** — what matters in the call
- **Considered options** — alternatives, briefly
- **Decision outcome** — what we picked + why
- **Positive / Negative consequences**
- **Links** — issues, prior art, related ADRs

Keep ADRs short. The point is a durable trace, not a research paper. If it grows past ~250 lines, split or scope down.

## Append-only discipline

Once Accepted, do **not** edit substance. To change direction:

1. Write a new ADR that supersedes the old one.
2. Update the old ADR's Status line to `Superseded by ADR-NNNN`.
3. Add a back-link to the new ADR.

Editing typos and formatting is fine. Editing decisions is not.
