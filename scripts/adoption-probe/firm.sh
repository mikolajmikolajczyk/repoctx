#!/usr/bin/env bash
# Firm-up sweep: REPEATS=3 across two corpora (madside TS + heartwood Rust).
# run.sh clobbers its own out/ each invocation, so snapshot per corpus.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

echo "=== [1/2] madside x3 ==="
REPEATS=3 "$HERE/run.sh" /home/mikolaj/src/madside 0
cp -r "$HERE/out" "$HERE/out-madside"
cp "$HERE/out/summary.txt" "$HERE/summary-madside.txt"

echo "=== [2/2] heartwood x3 ==="
REPEATS=3 TASKS_FILE="$HERE/tasks-heartwood.txt" "$HERE/run.sh" /home/mikolaj/src/heartwood 0
cp -r "$HERE/out" "$HERE/out-heartwood"
cp "$HERE/out/summary.txt" "$HERE/summary-heartwood.txt"

echo "=== DONE ==="
echo "--- madside ---";   cat "$HERE/summary-madside.txt"
echo "--- heartwood ---"; cat "$HERE/summary-heartwood.txt"
