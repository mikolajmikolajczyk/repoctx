#!/usr/bin/env bash
# Adoption probe: does guidance / session-start priming flip an agent's tool
# choice from grep/cat to repoctx? Runs each task headless (`claude -p`) under
# three arms and saves transcripts; `summarize.py` tallies them.
#
#   bare     — NO repoctx guidance: skill dir + CLAUDE.md repoctx block removed,
#              no SessionStart hook. The true grep baseline. (Backed up +
#              restored in place via a trap — source repos are too big to copy.)
#   guidance — committed guidance (skill + CLAUDE.md block) present, no hook.
#   primed   — guidance + SessionStart hook injecting `repoctx prime`.
#
# Usage: scripts/adoption-probe/run.sh <target-repo> [N_TASKS]
#   REPEATS=<k>      env runs each task k times (default 1) to see past nondeterminism.
#   TASKS_FILE=<f>   env picks the task corpus (default tasks.txt; e.g. tasks-heartwood.txt).
#
# Real `claude -p` runs — costs tokens. Read-only nav tasks;
# --dangerously-skip-permissions so tools execute headless.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "${1:?usage: run.sh <target-repo> [N]}" && pwd)"
LIMIT="${2:-0}"
REPEATS="${REPEATS:-1}"
OUT="$HERE/out"
rm -rf "$OUT"; mkdir -p "$OUT"

TASKS_FILE="${TASKS_FILE:-$HERE/tasks.txt}"   # override per corpus, e.g. tasks-heartwood.txt
mapfile -t TASKS < <(grep -vE '^\s*#|^\s*$' "$TASKS_FILE")
[ "$LIMIT" -gt 0 ] && TASKS=("${TASKS[@]:0:$LIMIT}")
echo "repo: $REPO   corpus: $(basename "$TASKS_FILE")   tasks: ${#TASKS[@]}   repeats: $REPEATS   arms: bare guidance primed"

repoctx --repo "$REPO" index >/dev/null 2>&1 || true

# --- bare-arm guidance backup/restore (in place; restored on any exit) ---
BK="$(mktemp -d)"
restore_guidance() {
  [ -f "$BK/CLAUDE.md" ] && cp "$BK/CLAUDE.md" "$REPO/CLAUDE.md"
  [ -d "$BK/skill" ] && { rm -rf "$REPO/.claude/skills/repoctx"; mkdir -p "$REPO/.claude/skills"; cp -r "$BK/skill" "$REPO/.claude/skills/repoctx"; }
  repoctx --repo "$REPO" init --yes >/dev/null 2>&1 || true   # leave primed
  return 0
}
trap restore_guidance EXIT

strip_guidance() {
  cp "$REPO/CLAUDE.md" "$BK/CLAUDE.md" 2>/dev/null || true
  [ -d "$REPO/.claude/skills/repoctx" ] && cp -r "$REPO/.claude/skills/repoctx" "$BK/skill"
  repoctx --repo "$REPO" init --uninstall >/dev/null 2>&1 || true
  rm -rf "$REPO/.claude/skills/repoctx"
  # delete the <!-- repoctx:start -->..<!-- repoctx:end --> block from CLAUDE.md
  [ -f "$REPO/CLAUDE.md" ] && python3 - "$REPO/CLAUDE.md" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
open(p, "w").write(re.sub(r"<!-- repoctx:start -->.*?<!-- repoctx:end -->\n?", "", s, flags=re.S))
PY
  return 0   # the trailing `[ -f ] && …` returns 1 when the repo has no CLAUDE.md,
             # which under `set -e` would kill the run before any task — guard it.
}

run_arm() { # arm
  local arm="$1"
  case "$arm" in
    bare)     strip_guidance ;;
    guidance) restore_guidance; repoctx --repo "$REPO" init --uninstall >/dev/null 2>&1 || true ;;
    # --force: a repo with a committed (older) skill makes plain `init --yes`
    # exit 1 ("destination exists with different content"), which under `set -e`
    # killed the sweep at the primed transition. Force-overwrite with the
    # current skill so the primed arm reflects this build, and `|| true` so a
    # nonzero init never aborts the run.
    primed)   restore_guidance; repoctx --repo "$REPO" init --yes --force >/dev/null 2>&1 || true ;;
  esac
  echo "  [$arm] running ${#TASKS[@]} tasks x $REPEATS ..."
  for i in "${!TASKS[@]}"; do
    for r in $(seq 1 "$REPEATS"); do
      # `timeout --kill-after`: a stalled headless run (intermittent API hang)
      # must not wedge the whole sweep. SIGTERM at PER_TIMEOUT, SIGKILL 15s later
      # so orphaned claude children die too. The run just yields a short/empty
      # transcript that summarize.py tallies as "no repoctx".
      ( cd "$REPO" && timeout --kill-after=15 "${PER_TIMEOUT:-180}" \
          claude -p "${TASKS[$i]}" \
          --output-format stream-json --verbose \
          --dangerously-skip-permissions \
          >"$OUT/${arm}-$(printf '%02d' "$i")-r${r}.jsonl" 2>/dev/null ) || true
    done
  done
}

for arm in bare guidance primed; do run_arm "$arm"; done
echo "done. transcripts in $OUT/"
python3 "$HERE/summarize.py" "$OUT" | tee "$OUT/summary.txt"
