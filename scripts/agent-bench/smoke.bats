#!/usr/bin/env bats
# Helper self-test — exercises the bench helpers against a tiny in-test
# fixture repo (no clone). Proves the harness wiring before the per-repo
# suites (helix / rust-analyzer / vuejs-core) run against real clones.
#
#   bats scripts/agent-bench/smoke.bats
#
# Requires the release binaries: `cargo build --release` first (or run
# via scripts/agent-bench/run.sh which builds them).

load lib/helpers

setup() {
  REPO="$(mktemp -d)"
  mkdir -p "$REPO/src"
  cat > "$REPO/src/widget.rs" <<'RS'
pub struct Widget { pub size: u32 }
impl Widget { pub fn build() -> Self { Widget { size: 0 } } }
RS
  # Several other files *reference* Widget — rg hits them all (the agent
  # would open each to find the definition); `definition` returns only the
  # real def site. This mirrors the real-repo asymmetry the bench measures.
  for n in a b c d e; do
    cat > "$REPO/src/$n.rs" <<RS
use crate::widget::Widget;
// $n uses Widget in several places to look like a real module.
fn ${n}_one(w: &Widget) -> u32 { w.size }
fn ${n}_two(w: &Widget) -> u32 { ${n}_one(w) + 1 }
fn ${n}_three(w: &Widget) -> u32 { ${n}_two(w) + 2 }
RS
  done
  cat > "$REPO/config.json" <<'JSON'
{ "name": "demo", "nested": { "inner": true } }
JSON
}

teardown() { rm -rf "$REPO"; }

@test "tokens binary counts bytes/4" {
  run bash -c "printf '12345678' | $TOKENS"
  [ "$status" -eq 0 ]
  [ "$output" -eq 2 ]
}

@test "definition beats rg-worst by a wide margin" {
  rc="$(repoctx_tokens "$REPO" definition Widget)"
  rg="$(rg_worst_tokens "$REPO" Widget)"
  assert_savings_above "$rc" "$rg" 50
}

@test "full-coverage hit carries no advisory" {
  assert_no_advisory "$REPO" definition Widget
}

@test "partial-coverage lang filter carries an advisory" {
  # json is partial coverage → --lang json must advise.
  assert_advisory_present "$REPO" symbols name --lang json
}

@test "savings_pct math" {
  run savings_pct 10 100
  [ "$output" -eq 90 ]
  run savings_pct 5 0
  [ "$output" -eq 0 ]
}

@test "callers: direct edge present and counted" {
  # a_two calls a_one -> callers(a_one) is exactly one edge.
  repoctx_json "$REPO" callers a_one | grep -q '"count":1'
}

@test "callgraph: transitive down reaches depth 2" {
  # a_three -> a_two -> a_one : a_one is reachable two hops down.
  repoctx_json "$REPO" callgraph a_three --direction down --depth 2 \
    | grep -q '"callee_name":"a_one"'
}

@test "search tags results with provenance (structural)" {
  # Wiring check: search emits the provenance-tagged stream with a
  # tree-sitter-confirmed structural item for the Widget struct.
  # (Real token savings are measured on the per-repo suites — on this toy
  # fixture the files are too small for search to beat rg-worst.)
  repoctx_json "$REPO" search Widget | grep -q '"source":"structural"'
}

@test "search includes a non-symbol (comment) mention" {
  # The widget.rs comment mentions Widget but isn't a symbol — search must
  # still surface it (no textual loss). repoctx symbols would drop it.
  printf '// a stray Widget mention in a comment\n' >> "$REPO/src/widget.rs"
  repoctx_json "$REPO" search Widget | grep -q 'stray Widget mention'
}
