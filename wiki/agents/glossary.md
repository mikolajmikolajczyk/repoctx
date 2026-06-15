# Glossary

Domain and project terminology. One term per entry. Keep definitions short — link out for depth.

- **backend** — any implementation of the `CodeIntelBackend` trait (ADR-0004). Today there's one: `TreeSitterBackend`.
- **call graph** — static, name-based map of which functions/methods call which, built from Tree-sitter syntax for the core-8 langs (ADR-0010). Queried via `callers`/`callees`/`callgraph`. Approximate — same accuracy class as `definition`.
- **call site / CallEdge** — a call-site row in the `calls` table (caller name+line, callee name, location, `resolution`); `CallEdge` is the backend type a query returns (caller `Symbol`, optional resolved callee, `ambiguous`).
- **callers / callees / callgraph** — CLI commands: who-calls (`callers`), what-it-calls (`callees`), transitive traversal (`callgraph --depth N --direction up|down|both`).
- **epic** — GitHub issue carrying the `epic` label; parent of `parent:#N`-labelled children.
- **resolution (call edge)** — how a call edge was derived: `'syntactic'` (Tree-sitter, name-based, today) or `'semantic'` (future LSP backend, ADR-0005), stored on the `calls` row so both fill one table.
- **search** — the `repoctx search` command: symbol definitions **+** every textual ripgrep match, compressed. Textually complete (no symbol-only loss); what the hook rewrites `rg <ident>` to.
- **issue #N** — a GitHub issue/PR number. The canonical short reference everywhere in this repo (labels, commit subjects, branch names).
- **incremental index** — re-index pass that reparses only files whose `(mtime_ns, size)` tuple changed (ADR-0006, ADR-0007).
- **labels** — GitHub label conventions for kanban-style boards (`state:*`, `priority:*`, `milestone:*`, `epic`, `parent:#N`, `blocked:*`).
- **state:in-progress** — GitHub label marking an issue currently being worked. See [`working-on-issues.md`](working-on-issues.md).
- **store** — the SQLite database at `.repoctx/index.db`, source of truth for files/symbols metadata (ADR-0003). Also the crate that owns it.
- **tags.scm** — Tree-sitter query file shipped by upstream grammars defining `@definition.*`/`@name` captures; our symbol-extraction source (ADR-0002).
- **TOON** — Token-Oriented Object Notation; default machine output format for non-TTY callers (ADR-0008). JSON via `--json`.
