#!/usr/bin/env bash
# Agent benchmark driver — repoctx vs ripgrep on real codebases.
#
# Manual only. Builds the release binaries, clones the pinned target repos
# (idempotent), and runs the bats suites. See
# wiki/decisions/2026-06-13-agent-bench.md for the design + thresholds.
#
#   scripts/agent-bench/run.sh            # build + smoke + any suites present
#   scripts/agent-bench/run.sh --clone    # also clone/refresh target repos
#
# Requires: bats (https://github.com/bats-core/bats-core), rg, git, cargo.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
clones="${BENCH_CLONES:-/tmp/repoctx-bench}"

# Pinned targets (repo  ref  dir) — ref is an immutable SHA or tag. Keep in
# sync with the design doc.
targets=(
  "helix-editor/helix       14eda106f0a3e6a5fc6fb5cbd96bda9774f64ae1 helix"
  "rust-lang/rust-analyzer  e79b8223f7e0f000d75e7bf9a8f5b590ff7eb7f8 rust-analyzer"
  "vuejs/core               478e3e83acd34dd213a860be4a2a2bf2090dc26b vuejs-core"
  "torvalds/linux           v6.6                                     linux"
)

echo "==> building release binaries"
( cd "$root" && cargo build --release -p repoctx -p repoctx-bench-tokens )
export REPOCTX="$root/target/release/repoctx"
export TOKENS="$root/target/release/tokens"

if [ "${1:-}" = "--clone" ]; then
  mkdir -p "$clones"
  for t in "${targets[@]}"; do
    read -r repo ref dir <<<"$t"
    dest="$clones/$dir"
    # Shallow fetch of just the pinned ref — full history of e.g. the Linux
    # kernel would be many GB; we only need one tree.
    if [ ! -d "$dest/.git" ]; then
      git init -q "$dest"
      git -C "$dest" remote add origin "https://github.com/$repo"
    fi
    echo "==> $dir fetch $ref"
    git -C "$dest" fetch -q --depth 1 origin "$ref"
    git -C "$dest" checkout -q FETCH_HEAD
  done
fi

if ! command -v bats >/dev/null 2>&1; then
  echo "bats not installed — see https://github.com/bats-core/bats-core" >&2
  exit 1
fi

echo "==> smoke (helper self-test)"
bats "$here/smoke.bats"

# Per-repo suites run when both the .bats file and the clone exist.
for t in "${targets[@]}"; do
  read -r _repo _sha dir <<<"$t"
  suite="$here/${dir}.bats"
  if [ -f "$suite" ] && [ -d "$clones/$dir" ]; then
    echo "==> suite: $dir"
    BENCH_REPO="$clones/$dir" bats "$suite"
  fi
done
