#!/usr/bin/env bats
# vuejs/core suite (TypeScript) — issue 866f929. Exercises the vendored
# TS/TSX tags.scm on a real codebase. Run via run.sh --clone.
# Thresholds per wiki/decisions/2026-06-13-agent-bench.md.

load lib/helpers

setup() {
  [ -n "${BENCH_REPO:-}" ] || skip "BENCH_REPO not set (run via run.sh)"
  [ -d "$BENCH_REPO" ] || skip "clone missing: $BENCH_REPO"
  "$REPOCTX" --repo "$BENCH_REPO" index >/dev/null 2>&1 || true
}

def_saves() { # <name>
  local rc rg
  rc="$(repoctx_tokens "$BENCH_REPO" definition "$1")"
  rg="$(rg_worst_tokens "$BENCH_REPO" "$1")"
  assert_savings_above "$rc" "$rg" 80
}

@test "definition: defineComponent" { def_saves defineComponent; }
@test "definition: reactive" { def_saves reactive; }
@test "definition: computed" { def_saves computed; }
@test "definition: effect" { def_saves effect; }

@test "context: watch" {
  rc="$(repoctx_tokens "$BENCH_REPO" context watch --limit 1 --context 6)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'function watch')"
  assert_savings_above "$rc" "$rg" 60
}

@test "symbols: ref" {
  rc="$(repoctx_tokens "$BENCH_REPO" symbols ref --limit 20)"
  rg="$(rg_worst_tokens "$BENCH_REPO" ref)"
  assert_savings_above "$rc" "$rg" 80
}

@test "outline: packages/compiler-sfc/src/parse.ts" {
  rc="$(repoctx_tokens "$BENCH_REPO" outline packages/compiler-sfc/src/parse.ts)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'export function')"
  assert_savings_above "$rc" "$rg" 80
}

@test "search: computed" {
  rc="$(repoctx_tokens "$BENCH_REPO" search computed --limit 30)"
  rg="$(rg_worst_tokens "$BENCH_REPO" computed)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callers: computed" {
  # Call graph (ADR-0010). Measured 99% vs rg-worst (v0.9.0).
  rc="$(repoctx_tokens "$BENCH_REPO" callers computed --limit 50)"
  rg="$(rg_worst_tokens "$BENCH_REPO" computed)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callees: computed" {
  rc="$(repoctx_tokens "$BENCH_REPO" callees computed --limit 50)"
  rg="$(rg_worst_tokens "$BENCH_REPO" computed)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callgraph: computed down depth 2" {
  rc="$(repoctx_tokens "$BENCH_REPO" callgraph computed --direction down --depth 2)"
  rg="$(rg_worst_tokens "$BENCH_REPO" computed)"
  assert_savings_above "$rc" "$rg" 90
}

@test "TS definition carries no advisory (full coverage)" {
  assert_no_advisory "$BENCH_REPO" definition reactive
}
