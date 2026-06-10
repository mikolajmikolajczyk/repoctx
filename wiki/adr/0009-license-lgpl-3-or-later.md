# ADR-0009 — License: LGPL-3.0-or-later

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: license, distribution

## Context

`repoctx` is a CLI tool likely to be invoked from larger workflows, some of which may be proprietary (e.g. internal automation, AI-agent harnesses). The license choice has to balance keeping improvements to `repoctx` itself open against permitting integration into closed workflows.

## Decision drivers

- Keep modifications to `repoctx` itself open (copyleft on the work, not on the caller).
- Allow integration into proprietary pipelines without forcing them open.
- Consistency with `pebble` (same author, same intent).
- Avoid AGPL because invocation-as-network-service is not the primary distribution model.

## Considered options

1. **LGPL-3.0-or-later** — weak copyleft; modifications to `repoctx` stay open, integrators stay free.
2. **AGPL-3.0-or-later** — strong copyleft including network use; overkill for a CLI.
3. **MIT / Apache-2.0** — permissive; allows closed forks of `repoctx` itself.
4. **Proprietary** — incompatible with the project's intent.

## Decision outcome

**LGPL-3.0-or-later.** Canonical text lives in `LICENSE` at repo root. The `-or-later` clause allows downstream users to migrate to a future, compatible LGPL revision without our intervention.

## Positive consequences

- Modifications to `repoctx` itself remain open.
- Proprietary callers can invoke or link `repoctx` (e.g. via exec) without license contagion to their own code.
- Matches sibling-project precedent.

## Negative consequences

- Some corporate adopters prefer MIT/Apache for any dependency; LGPL may slow adoption in the most license-averse environments.
- LGPL semantics for "linking" a Rust binary are less universally understood than they are for C; expect the occasional question.

## Links

- `LICENSE` — canonical text.
- README — license section.
