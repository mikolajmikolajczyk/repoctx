# Onboarding moved to `repoctx init`

> **This page has moved.** repoctx no longer ships a `PreToolUse` rewrite
> hook or a `repoctx hook` command. Onboarding is now a single command,
> `repoctx init`, documented in **[`init.md`](init.md)**.

## What changed

Earlier versions installed repoctx as a `PreToolUse → Bash` hook that
rewrote `rg`/`grep`/`find` to repoctx commands (with rtk chaining, a
committed `.repoctx/hook.sh` script, a `doctor` drift-checker, and
`discover` telemetry). That whole subsystem was removed — real-session
data showed it converted almost no agent traffic, and most of it was
already compressed by `rtk`.

Adoption is now via **session-start priming**: `repoctx init` (for
Claude) wires a **SessionStart** hook that runs [`repoctx prime`](commands.md#repoctx-prime),
injecting a compact repo orientation digest into the agent's context once
per session. repoctx no longer touches `PreToolUse`, so your own `rtk`
(or other) hook runs independently.

See [`2026-06-17-remove-pretooluse-hook`](../decisions/2026-06-17-remove-pretooluse-hook.md)
and [`2026-06-16-adoption-via-priming`](../decisions/2026-06-16-adoption-via-priming.md)
for the rationale.

## Where to go

- **[`init.md`](init.md)** — how to onboard (`repoctx init`, project vs
  global, Codex/opencode, uninstall).
- **[`commands.md`](commands.md#repoctx-prime)** — `repoctx prime`, the
  digest the SessionStart hook injects.
- **[`config.md`](config.md)** — per-repo settings.
