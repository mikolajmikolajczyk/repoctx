# Adoption via session-start priming, not per-command hook rewriting

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** product | adoption

## Context

`discover` telemetry showed the passive PreToolUse rewrite hook converts ~0% of
real agent traffic: the dominant shapes (`multi-term` `rg 'a|b'`, `explicit-path`,
`find`) are blocked by quoted shell metacharacters and a leading `cd <dir>;`
prefix the tokenizer refuses. Making the rewriter handle them means a quote-aware
shell tokenizer + cd-prefix stripping + a lossy `multi-term → search` mapping
(issue #11). Meanwhile most of that traffic is *already* compressed by the
chained `rtk` proxy, so the marginal token win of rewriting it is small — and the
real leverage (repoctx's structure-aware navigation) is something the agent has
to *choose* to use, which guidance in CLAUDE.md doesn't reliably trigger.

## Decision

Shift the adoption strategy from **intercepting every command** to **priming the
agent once per session**. A new `repoctx prime` emits a compact, token-budgeted
(~600 token) repo orientation digest — headline, top subsystems (#14), hubs,
entry points, and a skill pointer — generated deterministically from the index.
`repoctx hook install claude` registers it as a **SessionStart** hook so its
stdout lands in the agent's context at session start. The agent begins with a
structural map and a nudge to use `repoctx` instead of blind `grep`/`cat`.

`prime` never cold-indexes (it nudges if unindexed) so session start stays fast,
and refreshes incrementally otherwise. The full call graph is referenced by
command (`repoctx export`), never inlined — keeping the payload cheap.

The per-command rewrite hook stays as-is (it still helps the bare-ident case and
chains the rest to `rtk`); we simply stop investing in the brittle shell-parsing
expansion. Issue #11 is reframed: priming, not a quote-aware rewriter.

## Alternatives considered

- **Quote-aware tokenizer + cd-prefix + multi-term rewrite (original #11)** —
  brittle shell parsing for a small marginal win over `rtk`'s existing
  compression. Deferred, likely dropped.
- **Wire full `report` into SessionStart** — heavier payload, not budget-tuned
  for an every-session cost.
- **Inline the graph JSON** — 400+ nodes blows the token budget every session.

## Trigger to revisit

`discover` (or usage data) shows priming doesn't lift repoctx invocation, or the
digest's token cost outweighs the savings on short sessions → tune the budget or
reconsider the rewrite path.
