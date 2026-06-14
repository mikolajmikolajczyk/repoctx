# Deferred

Things **deliberately not implemented**. If something seems missing and is listed here, don't add it unprompted — there's a reason. Each entry: what, why deferred, when to revisit.

## Format

```markdown
### <Feature / behavior>

- **Why deferred:** <one paragraph>
- **Revisit when:** <trigger condition>
- **Tracked in:** <radicle issue hex7, if any>
```

## Entries

### LSP backend, `repoctxd` daemon, `refs`/`hover`

- **Why deferred:** type-aware semantic queries need warm long-lived LSP servers; ADR-0005 puts that in a separate daemon. The current Tree-sitter backend is the by-design first cut.
- **Revisit when:** Tree-sitter surface (index / symbols / outline / definition / context) is shipped and validated by real agent use.
- **Tracked in:** `58b45d5` (LSP-daemon placeholder epic).
- **Note:** `callers` is no longer in this entry — it is being un-deferred via a **static, name-based call graph** (accuracy class of `definition`, not LSP-grade) planned for v0.8.0, epic `af42572`, design recorded in [ADR-0010](../adr/0010-static-call-graph.md). `refs`/`hover` stay deferred to the LSP path.

### Fuzzy symbol matching

- **Why deferred:** `symbols` is case-insensitive substring via SQL LIKE — deterministic, cheap, good enough for agent callers. Fuzzy ranking adds a scoring dependency and nondeterministic ordering for unclear gain.
- **Revisit when:** real agent transcripts show substring misses being a problem.
- **Tracked in:** none.

### Content-hash cache invalidation

- **Why deferred:** `(mtime_ns, size)` is O(stat) and the failure mode is "stale answer", acceptable for a context tool (ADR-0006). `repoctx index --force` is the escape hatch.
- **Revisit when:** users report mtime-skew problems in practice.
- **Tracked in:** none — ADR-0006 records the call.

### Nested keys for JSON/YAML/TOML

- **Why deferred:** extractor pulls top-level keys only (kind `key`). Nested/dotted-path extraction multiplies symbol volume and needs a naming scheme nobody has asked for yet.
- **Revisit when:** an agent use case needs sub-document navigation in data files.
- **Tracked in:** none.

### Configurable file-size cap / skip rules

- **Why deferred:** 2 MiB cap and non-UTF-8 skip are hardcoded. Config surface (file or flags) is premature before real-world hits.
- **Revisit when:** the cap skips files users actually want indexed.
- **Tracked in:** none.

### Dynamic grammar loading / plugin system

- **Why deferred:** ADR-0002 statically links all 20 bundled grammars; plugin loading adds a trust and ABI surface with no current demand.
- **Revisit when:** a language outside the bundled set is needed by a real consumer.
- **Tracked in:** none — ADR-0002 records the call.

### Watch mode / filesystem notifications

- **Why deferred:** requires a daemon (`notify` crate + lifecycle); CLI-first per ADR-0001. Incremental `index` runs are cheap enough to call per-session.
- **Revisit when:** `repoctxd` exists — natural host for a watcher.
- **Tracked in:** `58b45d5`.
