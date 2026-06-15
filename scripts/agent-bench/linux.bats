#!/usr/bin/env bats
# linux-kernel suite (C, ~62k files / ~1.08M symbols) — the scale + C
# call-graph target. Run via scripts/agent-bench/run.sh --clone.
# Thresholds set from a real v0.9.0 clone run (see wiki/bench/results.md);
# every family measured 99% vs rg-worst, so the bars below carry margin.

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
  assert_savings_above "$rc" "$rg" 95
}

@test "definition: kmalloc" { def_saves kmalloc; }
@test "definition: schedule" { def_saves schedule; }
@test "definition: mutex_lock" { def_saves mutex_lock; }
@test "definition: task_struct" { def_saves task_struct; }

@test "context: schedule" {
  rc="$(repoctx_tokens "$BENCH_REPO" context schedule --limit 1 --context 6)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'void schedule')"
  assert_savings_above "$rc" "$rg" 90
}

@test "symbols: task_struct" {
  rc="$(repoctx_tokens "$BENCH_REPO" symbols task_struct --limit 20)"
  rg="$(rg_worst_tokens "$BENCH_REPO" task_struct)"
  assert_savings_above "$rc" "$rg" 95
}

@test "outline: kernel/sched/core.c" {
  rc="$(repoctx_tokens "$BENCH_REPO" outline kernel/sched/core.c)"
  rg="$(rg_worst_tokens "$BENCH_REPO" 'static')"
  assert_savings_above "$rc" "$rg" 95
}

@test "search: kmalloc" {
  rc="$(repoctx_tokens "$BENCH_REPO" search kmalloc --limit 30)"
  rg="$(rg_worst_tokens "$BENCH_REPO" kmalloc)"
  assert_savings_above "$rc" "$rg" 85
}

@test "callers: kmalloc" {
  rc="$(repoctx_tokens "$BENCH_REPO" callers kmalloc --limit 50)"
  rg="$(rg_worst_tokens "$BENCH_REPO" kmalloc)"
  assert_savings_above "$rc" "$rg" 90
}

@test "callgraph: schedule down depth 2" {
  rc="$(repoctx_tokens "$BENCH_REPO" callgraph schedule --direction down --depth 2)"
  rg="$(rg_worst_tokens "$BENCH_REPO" schedule)"
  assert_savings_above "$rc" "$rg" 90
}

@test "C definition carries no advisory (full coverage)" {
  assert_no_advisory "$BENCH_REPO" definition kmalloc
}

@test "suite indexes the kernel under budget" {
  run "$REPOCTX" --repo "$BENCH_REPO" status --fast
  [ "$status" -eq 0 ]
}
