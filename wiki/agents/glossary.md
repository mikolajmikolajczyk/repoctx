# Glossary

Domain and project terminology. One term per entry. Keep definitions short — link out for depth.

- **backend** — any implementation of the `CodeIntelBackend` trait (ADR-0004). M0 ships one: `TreeSitterBackend`.
- **epic** — Radicle issue carrying the `epic` label; parent of `parent:<hex7>`-labelled children. M0 epic: `e408787`.
- **hex7** — first 7 chars of a Radicle issue/patch object ID. The canonical short reference everywhere in this repo (labels, commit subjects, branch names).
- **incremental index** — re-index pass that reparses only files whose `(mtime_ns, size)` tuple changed (ADR-0006, ADR-0007).
- **radboard** — label-convention overlay on Radicle for kanban-style boards. See [`../../.agents/skills/radboard/SKILL.md`](../../.agents/skills/radboard/SKILL.md).
- **state:in-progress** — Radicle label marking an issue currently being worked. See [`working-on-issues.md`](working-on-issues.md).
- **store** — the SQLite database at `.repoctx/index.db`, source of truth for files/symbols metadata (ADR-0003). Also the crate that owns it.
- **tags.scm** — Tree-sitter query file shipped by upstream grammars defining `@definition.*`/`@name` captures; our symbol-extraction source (ADR-0002).
- **TOON** — Token-Oriented Object Notation; default machine output format for non-TTY callers (ADR-0008). JSON via `--json`.
