# ADR-0010 — Static, name-based call graph (Tree-sitter)

- **Status**: Accepted
- **Date**: 2026-06-14
- **Deciders**: Mikołaj Mikołajczyk
- **Tags**: backend, indexing, call-graph, schema, languages

## Context

A call graph — who-calls-X (`callers`), what-X-calls (`callees`), and transitive traversal (`callgraph`) — is the highest-value navigation query `repoctx` still lacks. It was deferred to the M2 LSP daemon (ADR-0002, ADR-0005, `wiki/agents/deferred.md`) on the grounds that *precise* edges need warm semantic servers (receiver-type resolution, dynamic dispatch, generics).

Three things have changed since that deferral:

- The Tree-sitter surface (`index`/`symbols`/`outline`/`definition`/`context`) shipped and was validated by real agent use (v0.7.0).
- `definition` already resolves by **name** and is accepted as useful despite being approximate — agents treat its output as a strong hint, not a proof.
- An agent's most common missing question is "who calls this?" — currently answered only by `rg`, which returns every textual match with no caller/callee structure.

This ADR records the decision to ship a **static, name-based** call graph now (epic `af42572`, milestone v0.8.0), overriding the LSP-only deferral for `callers` specifically.

## Decision drivers

- Match `repoctx`'s existing accuracy class: name-based, syntax-derived, advisory when ambiguous. No regression in promise.
- No daemon, no language toolchains — stay a single static binary, per ADR-0002.
- Do not fork the schema/CLI when the M2 LSP backend later produces precise edges — it must enrich, not replace.
- Reuse the existing `definition` name-resolution path rather than build a second resolver.

## Considered options

1. **Static name-based now; LSP enriches the same table later.** (chosen)
2. **Wait for the M2 LSP daemon** and ship nothing until precise edges exist.
3. **Heuristic type inference** (lightweight receiver-type guessing) on top of Tree-sitter — more accuracy, much more complexity, still not sound.

Option 2 leaves the highest-value query unanswered indefinitely. Option 3 buys partial accuracy at the cost of a fragile resolver that still needs the same advisory caveats — not worth it for the first cut.

## Decision outcome

**Ship a static, name-based call graph built from Tree-sitter syntax.**

### Resolution model

- **Call sites** are extracted from syntax via per-language Tree-sitter queries (a call expression / method-call node), the same way symbols are extracted today.
- **Caller** = the nearest enclosing function/method of the call site, found by walking **up the syntax tree** from the call node to the first callable-def ancestor, then matching it to a symbol by start line. (Tree-walk rather than symbol line-range containment: some grammars' `tags.scm` capture only the declarator line, not the whole body, so range containment misses body calls.)
- **Callee** = the called name, recorded as text. Resolution to symbol(s) — by the same name match `definition` uses — happens **at query time**, not at index time.
- A stored **edge** is `(file, caller_name, caller_start_line, callee_name, site_line, site_column, resolution)`. There is deliberately **no stored callee symbol id**: ids are reassigned on every reindex, so a cross-file id would dangle when the *other* file is reparsed. Callee resolution is a query-time join `callee_name → symbols(name)`.

### Edge model (the two shaping decisions)

- **Unresolved callees are stored, not dropped.** A call to a name with no in-repo definition (stdlib / external / dynamic) is still stored as an edge with `callee_name` set; the query-time join simply finds no symbol, so the API returns `callee: null`. `callees` thus shows external calls (e.g. `serde::from_str`) and the graph is complete.
- **Ambiguity is realized at query time, one result row per candidate.** A stored edge records the call site once; when `callee_name` resolves to N symbols (no receiver-type disambiguation — `a.foo()` and `b.foo()` both hit every `foo`), the resolving join yields N result rows, each flagged `ambiguous = true`. This preserves the "show every possible target" behaviour without storing redundant rows, and is robust to reindex. The original design contemplated storing one row per candidate; query-time expansion supersedes it (same observable result, no fragile stored ids).

### LSP-ready schema

The `calls` table (schema v4, child `a58cec1`) carries a **`resolution` column**: `'syntactic'` for these name-based edges, `'semantic'` for edges a future M2 LSP backend (epic `58b45d5`) will write into the **same table**. No schema fork: the LSP path adds precise `'semantic'` rows; the CLI and queries are identical regardless of source. Edges cascade-delete with their file (FK on `file_path`), so re-indexing a file replaces its edges atomically.

### Accuracy contract (name-based)

Stated to the agent, mirroring `definition`:

- No receiver-type disambiguation — every same-named target is a candidate.
- Dynamic dispatch, function pointers, and higher-order indirection are invisible.
- Cross-language edges are out of scope.
- Ambiguity (name → multiple symbols) and unresolved callees are surfaced through the existing **coverage-advisory** mechanism, so the agent knows when to fall back to `rg`.

### Scope

Core 8 languages first (Rust, Python, JS, TS, Go, C, C++, Java); remaining indexed languages are a follow-up child (`3412476`). Direct (`callers`/`callees`) and transitive (`callgraph --depth N --direction up|down|both`) queries. JSON/TOON/human output, gain-recorded like other read commands.

## Positive consequences

- Answers the top missing agent query now, with no daemon and no toolchains.
- Same accuracy class and advisory UX as `definition` — no new promise to break.
- One resolver: reuses the `definition` name-resolution path.
- Forward-compatible: the M2 LSP backend upgrades edge precision in place, no migration.

## Negative consequences

- Over-approximation: ambiguous calls fan out to all same-named targets; agents must read the advisory.
- No dynamic-dispatch / indirect-call coverage — a known, documented blind spot until the LSP path lands.
- The `calls` table grows with unresolved + ambiguous rows (accepted: edges are small, and completeness beats a sparse-but-clean graph for agent use).

## Links

- Epic `af42572` (static call graph) — this ADR is its design child `00c5883`.
- ADR-0002 (Tree-sitter primary backend) — this is the syntax-derived, name-based extension of that model; updates its "`callers` deferred" note.
- ADR-0005 (LSP via `repoctxd`) — the `'semantic'` resolution path that later fills the same `calls` table.
- `wiki/agents/deferred.md` — `callers` moved out of the LSP-only deferral (refs/hover stay deferred).
- Child issues: `a58cec1` (schema v4), `ec57ab9` (extraction), `53d9c7e` (backend trait), `979904c`/`520f710` (CLI).
