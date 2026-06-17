#!/usr/bin/env bash
# Adoption probe: does session-start priming flip an agent's tool choice from
# grep/cat to repoctx? Runs each task headless (`claude -p`) under two
# conditions — `control` (no SessionStart hook) and `primed` (`repoctx init`
# SessionStart hook injects `repoctx prime`) — and tallies the tool calls.
#
# Usage:
#   scripts/adoption-probe/run.sh <target-repo> [N]
#     <target-repo>  a repo `repoctx` can index (e.g. ~/src/madside)
#     N              number of tasks to run (default: all)
#
# Output: a per-condition table + the raw transcripts under
# scripts/adoption-probe/out/. Real `claude -p` runs — costs tokens. Read-only
# nav tasks; runs with --dangerously-skip-permissions so tools execute headless.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="${1:?usage: run.sh <target-repo> [N]}"
LIMIT="${2:-0}"
OUT="$HERE/out"
mkdir -p "$OUT"

mapfile -t TASKS < <(grep -vE '^\s*#|^\s*$' "$HERE/tasks.txt")
[ "$LIMIT" -gt 0 ] && TASKS=("${TASKS[@]:0:$LIMIT}")

echo "repo: $REPO   tasks: ${#TASKS[@]}   conditions: control, primed"
# Make sure the index exists so priming/repoctx have data + don't cold-index.
repoctx --repo "$REPO" index >/dev/null 2>&1 || true

run_one() { # condition task_index task_text
  local cond="$1" idx="$2" task="$3"
  local tx="$OUT/${cond}-$(printf '%02d' "$idx").jsonl"
  ( cd "$REPO" && claude -p "$task" \
      --output-format stream-json --verbose \
      --dangerously-skip-permissions >"$tx" 2>/dev/null ) || true
  python3 "$HERE/tally.py" <"$tx"
}

declare -A AGG
for cond in control primed; do
  if [ "$cond" = primed ]; then
    repoctx --repo "$REPO" init --yes >/dev/null 2>&1
  else
    repoctx --repo "$REPO" init --uninstall >/dev/null 2>&1 || true
  fi
  echo
  echo "== $cond =="
  printf '%-3s %-9s %-7s %-6s %-6s %s\n' "#" "repoctx" "grep" "find" "read" "used_repoctx"
  used=0; rc=0; sc=0
  for i in "${!TASKS[@]}"; do
    j=$(run_one "$cond" "$i" "${TASKS[$i]}")
    r=$(echo "$j" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["repoctx"])')
    g=$(echo "$j" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["grep"])')
    f=$(echo "$j" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["find"])')
    rd=$(echo "$j" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["read"]+d["native_search"])')
    u=$(echo "$j" | python3 -c 'import json,sys;print(1 if json.load(sys.stdin)["used_repoctx"] else 0)')
    printf '%-3s %-9s %-7s %-6s %-6s %s\n' "$i" "$r" "$g" "$f" "$rd" "$u"
    used=$((used+u)); rc=$((rc+r)); sc=$((sc+g+f+rd))
  done
  AGG[$cond]="used=$used/${#TASKS[@]}  repoctx_calls=$rc  search_calls=$sc"
done

echo
echo "== summary =="
for cond in control primed; do printf '%-8s %s\n' "$cond" "${AGG[$cond]}"; done
echo "(transcripts in $OUT/)"
