#!/usr/bin/env bats
# helix suite (Rust, ~150k LOC) — issue 255bac3.
# Run via scripts/agent-bench/run.sh --clone (sets BENCH_REPO + binaries).
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

@test "definition: Selection" { def_saves Selection; }
@test "definition: Editor" { def_saves Editor; }
@test "definition: Transaction" { def_saves Transaction; }
@test "definition: Application" { def_saves Application; }

@test "context: render" {
  rc="$(repoctx_tokens "$BENCH_REPO" context render --limit 1 --context 6)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'fn render')"
  assert_savings_above "$rc" "$rg" 60
}

@test "symbols: Transaction" {
  rc="$(repoctx_tokens "$BENCH_REPO" symbols Transaction --limit 20)"
  rg="$(rg_worst_tokens "$BENCH_REPO" Transaction)"
  assert_savings_above "$rc" "$rg" 80
}

@test "outline: helix-core/src/selection.rs" {
  rc="$(repoctx_tokens "$BENCH_REPO" outline helix-core/src/selection.rs)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'pub fn')"
  assert_savings_above "$rc" "$rg" 80
}

@test "search: Selection" {
  # Textually-complete search: defs + every match, compressed. Returns more
  # than `symbols` but still ~98% below rg-worst (measured v0.9.0).
  rc="$(repoctx_tokens "$BENCH_REPO" search Selection --limit 30)"
  rg="$(rg_worst_tokens "$BENCH_REPO" Selection)"
  assert_savings_above "$rc" "$rg" 85
}

@test "callers: render" {
  # Who calls render — rg-worst opens every file mentioning "render"; `callers`
  # returns just the edges. Measured 99% vs rg-worst.
  rc="$(repoctx_tokens "$BENCH_REPO" callers render --limit 50)"
  rg="$(rg_worst_tokens "$BENCH_REPO" render)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callees: render" {
  rc="$(repoctx_tokens "$BENCH_REPO" callees render --limit 50)"
  rg="$(rg_worst_tokens "$BENCH_REPO" render)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callgraph: render up depth 2" {
  rc="$(repoctx_tokens "$BENCH_REPO" callgraph render --direction up --depth 2)"
  rg="$(rg_worst_tokens "$BENCH_REPO" render)"
  assert_savings_above "$rc" "$rg" 90
}

@test "full-coverage definition carries no advisory" {
  assert_no_advisory "$BENCH_REPO" definition Selection
}
