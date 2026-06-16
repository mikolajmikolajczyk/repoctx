# Receiver-aware call resolution: is_method flag, method calls bind only to methods

**Date:** 2026-06-16
**Decider:** Mikołaj Mikołajczyk
**Tags:** algorithm | correctness | schema

## Context

Name-based resolution (ADR-0010) bound every `obj.foo()` to any symbol named
`foo`. A receiver-value method call (`map.set()`, `arr.push()`) therefore bound
to a lone same-named free `function set`, fabricating fake super-hubs that
dominated `hotspots`, god nodes, communities, and `report`. The interim fix was
a blanket `HOST_METHOD_NAMES` stop-list, which also dropped legitimate free
calls to functions named `create`/`join`/etc.

## Decision

Record at extraction whether each call carries a **receiver value** —
`is_method` (schema v9, `calls.is_method`). Detected purely from the
Tree-sitter node shape: the callee identifier's parent is a
member/field/attribute/selector node for receiver calls, and a plain/scoped/
qualified node (or a Java `method_invocation` without an `object` field) for
free/path calls. Resolution rule (`store::callee_match`):

- **method call** (`is_method = 1`) → resolves only to a `method`; never a free
  `function`. `Type::foo()` / `ns::foo()` are *path* calls (`is_method = 0`), so
  Rust associated functions and namespaced calls still resolve.
- **free/path call** (`is_method = 0`) → `function` / `method` / `macro`.
- A residual `BUILTIN_METHOD_NAMES` guard drops method calls to builtin names
  (`push`/`get`/`set`/…) even when a same-named repo *method* exists — a
  method→method collision that needs receiver *types* to resolve. Free calls and
  method calls to other names are unaffected (strictly looser than the old list).

This replaces the blanket stop-list with a precise rule; existing rows default
`is_method = 0` until `repoctx index --force`.

## Alternatives considered

- **Keep the blanket HOST_METHOD_NAMES stop-list** — drops legit free calls to
  those names; can't tell method from function. Rejected (superseded).
- **Full receiver-type resolution** — the correct fix for the residual
  method→method collision, but needs a type model / LSP backend. Deferred.
- **Compute is_method at query time** — the call node isn't in the DB; would
  require reparsing. Storing one bool per edge is cheaper.

## Trigger to revisit

A semantic (LSP) backend or a local type/receiver model lands → resolve method
calls by receiver type and drop `BUILTIN_METHOD_NAMES` entirely.
