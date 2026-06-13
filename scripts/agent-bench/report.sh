#!/usr/bin/env bash
# Emit the per-query benchmark numbers as a markdown table — feeds the
# results page (wiki/bench/results.md). Pass/fail lives in the .bats
# suites; this is the "get the numbers" companion.
#
#   BENCH_CLONES=/tmp/repoctx-bench scripts/agent-bench/report.sh
#
# Requires the clones to exist already (run.sh --clone). Uses the same
# bytes/4 metric + helpers as the suites.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
clones="${BENCH_CLONES:-/tmp/repoctx-bench}"

export REPOCTX="${REPOCTX:-$root/target/release/repoctx}"
export TOKENS="${TOKENS:-$root/target/release/tokens}"
# shellcheck source=lib/helpers.bash
source "$here/lib/helpers.bash"

ver="$("$REPOCTX" --version | awk '{print $2}')"
echo "repoctx $ver — $(date -I) — metric: bytes/4"
echo

# row <repo> <family> <cmd...> | <rg-pattern>
# We split args from the rg pattern on the literal '||'.
row() {
  local repo="$1" family="$2"; shift 2
  local args=() pat=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "||" ]; then shift; pat="$*"; break; fi
    args+=("$1"); shift
  done
  local rc rgw rgb
  rc="$(repoctx_tokens "$repo" "${args[@]}")"
  rgw="$(rg_worst_tokens "$repo" "$pat")"
  rgb="$(rg_best_tokens "$repo" "$pat")"
  printf '| %s | `%s` | %s | %s | %s | %s%% | %s%% |\n' \
    "$family" "${args[*]}" "$rc" "$rgw" "$rgb" \
    "$(savings_pct "$rc" "$rgw")" "$(savings_pct "$rc" "$rgb")"
}

suite() { # <dir> <header> then rows on stdin via the functions below
  local dir="$1" header="$2" repo="$clones/$1"
  [ -d "$repo" ] || { echo "_skip $dir (no clone)_"; echo; return; }
  "$REPOCTX" --repo "$repo" index >/dev/null 2>&1 || true
  echo "### $header"
  echo
  echo "| Family | Query | repoctx | rg-worst | rg-best | save vs worst | save vs best |"
  echo "|---|---|--:|--:|--:|--:|--:|"
  REPO="$repo"
}

# Convenience wrappers bound to the current $REPO (set by suite()).
d()  { row "$REPO" definition definition "$1" "||" "$1"; }
ctx(){ row "$REPO" context context "$1" --limit 1 --context 6 "||" "$2"; }
sy() { row "$REPO" symbols symbols "$1" --limit 20 "||" "$1"; }
ol() { row "$REPO" outline outline "$1" "||" "$2"; }

suite helix "helix-editor/helix @ 14eda10 (Rust ~150k LOC)"
d Selection; d Editor; d Transaction; d Application
ctx render 'fn render'
sy Transaction
ol helix-core/src/selection.rs 'pub fn'
echo

suite vuejs-core "vuejs/core @ 478e3e8 (TypeScript)"
d defineComponent; d reactive; d computed; d effect
ctx watch 'function watch'
sy ref
ol packages/compiler-sfc/src/parse.ts 'export function'
echo

suite rust-analyzer "rust-lang/rust-analyzer @ e79b822 (Rust ~500k LOC)"
d Semantics; d SourceFile; d Analysis; d AssistContext
ctx completions 'fn completions'
sy Completions
ol crates/ide/src/lib.rs 'pub fn'
echo
