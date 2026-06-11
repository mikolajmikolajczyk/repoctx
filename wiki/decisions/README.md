# Decision log

Mid-weight decisions that don't qualify as ADRs but are too durable for a single commit message or PR description. Append-only, lightweight, grep-friendly.

## When to write here vs ADR vs commit / issue comment

| Where | When |
|-------|------|
| **ADR** (`../adr/`) | Constrains app shape or public contracts. Hard to reverse. Affects future contributors. Examples: layering rules, plugin host model, error boundary strategy, schema version, public interface shape. |
| **Decision log** (this folder) | Cross-cutting tool / library / process choice not tied to one issue. Reversible in days, not months. Examples: "we use library A over B for role X", "generated artifacts checked in not built per-CI", "AI agents in this repo write commit messages but never push". |
| **Issue comment** | Decision tied to a specific issue. Found via `rad issue show <hex7>`. |
| **Commit message body** | Decision tied to a specific commit. Examples: "switched from sha256 to sha1 for blob hashing — IDB key length, no collision risk at our scale". |

See [`../adr/README.md`](../adr/README.md) for the ADR bar in detail.

## Format

One markdown file per decision. Name: `YYYY-MM-DD-short-slug.md`. Keep each entry under ~50 lines — long entries probably want to be ADRs.

Template:

```markdown
# <One-line decision summary>

**Date:** YYYY-MM-DD
**Decider:** <name>
**Tags:** library-choice | process | tooling | ...

## Context

What prompted the decision. One paragraph.

## Decision

What we picked. One paragraph.

## Alternatives considered

- **Option A** — short reason it lost
- **Option B** — short reason it lost

## Trigger to revisit

What would make us re-open this decision.
```

## Index

- [2026-06-11 — Grammar crates: toml-ng + md, core 0.25.x](2026-06-11-grammar-crate-selection.md)
- [2026-06-11 — Platform-agnostic from the start: Linux, macOS, Windows](2026-06-11-platform-agnostic.md)
