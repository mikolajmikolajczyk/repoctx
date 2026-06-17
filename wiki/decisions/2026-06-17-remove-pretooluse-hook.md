# Remove the per-command PreToolUse rewrite hook

**Date:** 2026-06-17
**Decider:** Mikołaj Mikołajczyk
**Tags:** product | adoption | removal

## Context

repoctx shipped a PreToolUse Bash hook that intercepted `grep`/`rg`/`find` and
rewrote them to `repoctx` commands, with rtk chaining, a committed
`.repoctx/hook.sh` script, settings.json takeover, a `doctor` drift-checker, and
`discover` telemetry. `discover` data (from real sessions) showed it converted
**~0%** of agent traffic: the dominant shapes (multi-term `rg 'a|b'`,
explicit-path, find) were blocked by quoted shell metacharacters and a leading
`cd <dir>;` prefix the tokenizer refused, and **most of that traffic was already
compressed by the chained `rtk` proxy** — so the marginal token win of fixing
the rewriter was small. The adoption pivot to session-start priming
(2026-06-16-adoption-via-priming) made the whole PreToolUse subsystem redundant.

## Decision

Delete the per-command rewrite hook entirely (~2,800 LOC):
`hook_rewrite`/`hook_scan`/`hook_script`/`hook_marker`/`discover_cmd`/`hook_cmd`
modules, the `repoctx hook`/`rewrite`/`discover` commands, the `hook.sh`
template, rtk chaining, settings takeover/doctor, the `hook_events`/`hook_samples`
telemetry methods, and the `hook.*` config keys. `repoctx init` becomes the
single onboarding command (guidance files + SessionStart prime hook); the
SessionStart wiring lives in `session_hook.rs`. Old `hook.*` settings rows are
ignored silently; `config set hook.*` reports them obsolete. The SQL telemetry
tables are left in place (migrations are append-only) but unused.

repoctx no longer touches PreToolUse at all — a user's own `rtk` (or other) hook
operates independently.

## Alternatives considered

- **Keep it, fix the tokenizer (quote-aware + cd-prefix)** — brittle shell
  parsing for a marginal win over rtk's existing compression. Rejected (#11).
- **Keep `discover` repointed at prime effectiveness** — its data source (the
  hook) is gone; `gain` already measures real `repoctx` command usage. Rejected.
- **Keep `hook.*` config for back-compat** — dead surface; silent-ignore of old
  rows is enough.

## Trigger to revisit

Priming proves insufficient AND a low-brittleness interception path appears
(e.g. a structured tool-call API rather than raw shell parsing).
