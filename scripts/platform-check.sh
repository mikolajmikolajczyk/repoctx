#!/usr/bin/env bash
# Enforce platform-agnostic constraints across crates/.
#
# Fails (exit 1) on:
#   - `std::os::unix` / `cfg(unix)` / `cfg(target_os = ...)` in M0/M1 crates
#   - `MAIN_SEPARATOR` (use the store's to_db_path/from_db_path instead)
#   - scattered separator munging (replace('\\', "/") and friends)
#
# Wiki/decisions: 2026-06-11-platform-agnostic.md
#
# Run from the repo root.

set -euo pipefail

violations=0

scan() {
    local label="$1" pattern="$2"
    local hits
    hits=$(grep -RnE "$pattern" crates/ 2>/dev/null || true)
    if [ -n "$hits" ]; then
        echo "[FAIL] $label"
        echo "$hits"
        violations=$((violations + 1))
    else
        echo "[ ok ] $label"
    fi
}

scan "no std::os::unix"          'std::os::unix'
scan "no cfg(unix)"              'cfg\(unix\)'
scan "no cfg(target_os)"         'cfg\(target_os'
scan "no MAIN_SEPARATOR"         'MAIN_SEPARATOR'
scan "no '\\\\' -> '/' munging"  "replace\\(.{1,4}\\\\\\\\.{1,4},"

if [ "$violations" -gt 0 ]; then
    echo
    echo "platform-check: $violations rule(s) violated"
    exit 1
fi

echo "platform-check: all rules passed"
