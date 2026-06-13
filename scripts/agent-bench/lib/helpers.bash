# Shared bats helpers for the repoctx agent benchmark.
#
# Metric: bytes/4 (see wiki/decisions/2026-06-13-agent-bench.md), computed
# by the `tokens` binary so the .bats files only call shell — no Python.
#
# Env:
#   REPOCTX  path to the repoctx binary (default: target/release/repoctx)
#   TOKENS   path to the tokens binary   (default: target/release/tokens)
#   BENCH_REPO  the repo under test (a checked-out target clone)

: "${REPOCTX:=target/release/repoctx}"
: "${TOKENS:=target/release/tokens}"

# repoctx_tokens <repo> <args...> — token count of `repoctx --json <args>`.
# Runs from inside the repo so file-path args (outline) resolve.
repoctx_tokens() {
  local repo="$1"; shift
  ( cd "$repo" && "$REPOCTX" --json "$@" 2>/dev/null ) | "$TOKENS"
}

# repoctx_json <repo> <args...> — raw JSON stdout (for advisory asserts).
repoctx_json() {
  local repo="$1"; shift
  ( cd "$repo" && "$REPOCTX" --json "$@" 2>/dev/null )
}

# rg_worst_tokens <repo> <pattern> — rg match output + bytes/4 of EVERY
# candidate file (the cost if the agent opens all matches).
rg_worst_tokens() {
  local repo="$1" pat="$2" total=0 f
  local match_bytes
  match_bytes=$( (cd "$repo" && rg -n "$pat" 2>/dev/null) | wc -c )
  total=$(( match_bytes / 4 ))
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    total=$(( total + $(wc -c < "$repo/$f" 2>/dev/null || echo 0) / 4 ))
  done < <(cd "$repo" && rg -l "$pat" 2>/dev/null)
  echo "$total"
}

# rg_best_tokens <repo> <pattern> — rg match output + bytes/4 of just the
# top-match file (the optimistic case).
rg_best_tokens() {
  local repo="$1" pat="$2" top
  local match_bytes
  match_bytes=$( (cd "$repo" && rg -n "$pat" 2>/dev/null) | wc -c )
  top=$( (cd "$repo" && rg -l "$pat" 2>/dev/null) | head -1 )
  local file_tok=0
  [ -n "$top" ] && file_tok=$(( $(wc -c < "$repo/$top" 2>/dev/null || echo 0) / 4 ))
  echo $(( match_bytes / 4 + file_tok ))
}

# savings_pct <repoctx_tok> <rg_tok> — integer percent saved (0 if rg is 0).
savings_pct() {
  local rc="$1" rg="$2"
  if [ "$rg" -le 0 ]; then echo 0; return; fi
  awk "BEGIN { printf \"%d\", 100 * ($rg - $rc) / $rg }"
}

# assert_savings_above <repoctx_tok> <rg_tok> <min_pct>
assert_savings_above() {
  local rc="$1" rg="$2" min="$3"
  # Guard against a broken query: empty repoctx output is 0 tokens, which
  # would read as 100% savings and pass spuriously. A real query returns
  # at least a few tokens.
  if [ "$rc" -le 0 ]; then
    echo "repoctx produced no output (0 tokens) — broken query, not a win" >&2
    return 1
  fi
  if [ "$rg" -le 0 ]; then
    echo "ripgrep produced no candidates (0 tokens) — query found nothing" >&2
    return 1
  fi
  local pct; pct=$(savings_pct "$rc" "$rg")
  if [ "$pct" -lt "$min" ]; then
    echo "savings ${pct}% < required ${min}% (repoctx=$rc rg=$rg)" >&2
    return 1
  fi
}

# assert_advisory_present <repo> <args...>
assert_advisory_present() {
  local repo="$1"; shift
  if ! repoctx_json "$repo" "$@" | grep -q '"advisory"'; then
    echo "expected an advisory for: $* (repo=$repo)" >&2
    return 1
  fi
}

# assert_no_advisory <repo> <args...>
assert_no_advisory() {
  local repo="$1"; shift
  if repoctx_json "$repo" "$@" | grep -q '"advisory"'; then
    echo "unexpected advisory for: $* (repo=$repo)" >&2
    return 1
  fi
}
