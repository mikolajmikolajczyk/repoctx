#!/usr/bin/env bats
# rust-analyzer suite (Rust, ~500k LOC, ~1500 files) — large-repo stress,
# issue 22b09a3. Run via run.sh --clone.
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

@test "definition: Semantics" { def_saves Semantics; }
@test "definition: SourceFile" { def_saves SourceFile; }
@test "definition: Analysis" { def_saves Analysis; }
@test "definition: AssistContext" { def_saves AssistContext; }

@test "context: completions" {
  rc="$(repoctx_tokens "$BENCH_REPO" context completions --limit 1 --context 6)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'fn completions')"
  assert_savings_above "$rc" "$rg" 60
}

@test "symbols: Completions" {
  rc="$(repoctx_tokens "$BENCH_REPO" symbols Completions --limit 20)"
  rg="$(rg_worst_tokens "$BENCH_REPO" Completions)"
  assert_savings_above "$rc" "$rg" 80
}

@test "outline: crates/ide/src/lib.rs" {
  rc="$(repoctx_tokens "$BENCH_REPO" outline crates/ide/src/lib.rs)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'pub fn')"
  assert_savings_above "$rc" "$rg" 80
}

@test "full-coverage definition carries no advisory" {
  assert_no_advisory "$BENCH_REPO" definition Semantics
}

@test "suite indexes the large repo under budget" {
  run "$REPOCTX" --repo "$BENCH_REPO" status --fast
  [ "$status" -eq 0 ]
}
