#!/usr/bin/env bash
# rtk fidelity canary — does the rtk chain corrupt any command repoctx hands it?
#
# Manual only. Never wired into CI (depends on the locally installed rtk and
# on real tools being present). Run it after an rtk version bump, or whenever
# you suspect the chain is mangling output.
#
#   scripts/rtk-fidelity/run.sh
#
# For each probe command it drives the *real production path* — pipes the
# command through `repoctx hook claude --rtk-chain=1` and classifies the
# decision:
#
#   BYPASS    repoctx returns passthrough → the real tool runs untouched
#             (e.g. flagged `rg`). Always safe; reported, never gated.
#   SEMANTIC  repoctx rewrote it to a `repoctx <cmd>` (e.g. `rg foo` →
#             `repoctx symbols`). repoctx owns the output; out of scope here.
#   CHAIN     repoctx handed it to rtk. THIS is the fidelity gate: we run the
#             rtk rewrite and the real command and compare.
#
# Gate verdicts on CHAIN commands:
#   FAIL  rtk produced NO output while the real command produced some — the
#         silent false-empty class (what broke `ls` in rtk <=0.41). Hard fail.
#   WARN  rtk output is drastically smaller than the real output AND carries
#         no truncation marker — a possible silent drop; eyeball it.
#   PASS  rtk output is non-empty and either comparable or clearly signals
#         truncation (intended compression — the whole point of the chain).
#
# Exit non-zero if any CHAIN command FAILs, so it can gate a release check.
#
# Requires: repoctx + rtk on PATH (or set REPOCTX), python3, and the probed
# tools (ls, cat, find, tree, git, grep, diff, wc, head, tail).
set -uo pipefail

REPOCTX="${REPOCTX:-repoctx}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
cd "$root" || exit 1

if ! command -v "$REPOCTX" >/dev/null 2>&1; then
  echo "rtk-fidelity: repoctx not found (set REPOCTX=/path)" >&2; exit 2
fi
if ! command -v rtk >/dev/null 2>&1; then
  echo "rtk-fidelity: rtk not found on PATH" >&2; exit 2
fi

fixture="$(mktemp -d)"
printf 'alpha\nbeta\ngamma\n'  > "$fixture/a.txt"
printf 'alpha\nBETA\ngamma\n'  > "$fixture/b.txt"
trap 'rm -rf "$fixture"' EXIT

# Probe commands. Edit/extend freely — one original command per line. Add the
# proxies relevant to your environment (docker, kubectl, gh, cargo, …) where
# those tools exist.
#
# A leading "~" marks a proxy that *intentionally* compresses heavily and is
# not line-comparable to the raw tool (e.g. `tree` is gitignore-aware while
# raw `tree` walks target/ and .git). For those the ratio-WARN is skipped but
# the false-empty FAIL still applies — we still catch it going silent.
probes=(
  "ls"
  "ls -la"
  "cat README.md"
  "head -5 README.md"
  "tail -5 README.md"
  "wc -l README.md"
  "find . -name '*.rs'"
  "~tree"
  "git status"
  "git log --oneline -5"
  "grep -rn fn crates"
  "diff $fixture/a.txt $fixture/b.txt"
  "rg foo"
  "rg -i foo"
  "env"
)

# A truncation/summary marker means rtk told the agent it elided output — that
# is honest compression, not silent loss.
marker_re='more|truncat|total:|\.\.\.|…|\+[0-9]+|[0-9]+ (more|lines)|[0-9]+F |[0-9]+D '

# rtk's false-empty failure prints a sentinel, not zero bytes (rtk <=0.41's
# `ls` emitted the literal "(empty)"). Treat blank/whitespace and that
# sentinel as empty so the FAIL gate fires.
effectively_empty() {
  local s; s="$(printf '%s' "$1" | tr -d '[:space:]')"
  [ -z "$s" ] || [ "$s" = "(empty)" ]
}

decision() { # orig -> prints "BYPASS" | "SEMANTIC <cmd>" | "CHAIN <cmd>"
  local orig="$1" out
  out="$(printf '%s' "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$orig\"}}" \
        | "$REPOCTX" hook claude --rtk-chain=1 2>/dev/null)"
  if [ -z "$out" ]; then echo "BYPASS"; return; fi
  local rw
  rw="$(printf '%s' "$out" | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["hookSpecificOutput"]["updatedInput"]["command"])
except Exception: print("")' 2>/dev/null)"
  case "$rw" in
    "repoctx "*) echo "SEMANTIC $rw" ;;
    "") echo "BYPASS" ;;
    *) echo "CHAIN $rw" ;;
  esac
}

echo "rtk-fidelity canary — $(rtk -V 2>/dev/null) — repoctx $("$REPOCTX" --version 2>/dev/null | awk '{print $2}')"
echo

fails=0 warns=0
for entry in "${probes[@]}"; do
  heavy=0; orig="$entry"
  case "$entry" in "~"*) heavy=1; orig="${entry#\~}" ;; esac
  d="$(decision "$orig")"; verb="${d%% *}"; rw="${d#* }"
  case "$verb" in
    BYPASS)
      printf '  BYPASS   %s\n' "$orig" ;;
    SEMANTIC)
      printf '  SEMANTIC %-28s -> %s\n' "$orig" "$rw" ;;
    CHAIN)
      rtk_out="$(eval "$rw" 2>/dev/null)";  rl=$(printf '%s' "$rtk_out" | grep -c '' )
      tru_out="$(eval "$orig" 2>/dev/null)"; tl=$(printf '%s' "$tru_out" | grep -c '' )
      [ -z "$rtk_out" ] && rl=0; [ -z "$tru_out" ] && tl=0
      if effectively_empty "$rtk_out" && [ "$tl" -gt 0 ]; then
        printf '  FAIL     %-28s rtk=EMPTY real=%s lines  (silent false-empty)\n' "$orig" "$tl"
        fails=$((fails+1))
      elif [ "$heavy" -eq 0 ] && [ "$tl" -gt 0 ] && [ "$rl" -lt $((tl/4)) ] && ! printf '%s' "$rtk_out" | grep -Eq "$marker_re"; then
        printf '  WARN     %-28s rtk=%s real=%s lines, no truncation marker\n' "$orig" "$rl" "$tl"
        warns=$((warns+1))
      else
        printf '  PASS     %-28s rtk=%s real=%s lines\n' "$orig" "$rl" "$tl"
      fi ;;
  esac
done

echo
echo "rtk-fidelity: $fails fail, $warns warn"
[ "$fails" -eq 0 ]
