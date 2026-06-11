#!/usr/bin/env bash
# Performance benchmarks for repoctx (Radicle issue 948b131).
#
# Synthesizes a 5,000-file mixed-language corpus in a tempdir, then runs
# hyperfine against four scenarios with hard budgets. Exits non-zero on
# budget breach. Manual gate — NOT in CI (runner variance would flake).
#
# Usage: scripts/bench.sh [--build] [--no-cleanup]
#   --build       run `cargo build --release` first (default: assume current)
#   --no-cleanup  leave the synthetic corpus on disk for inspection
#
# Requires: hyperfine, cargo, jq.

set -euo pipefail

CARGO_BUILD=0
NO_CLEANUP=0
for arg in "$@"; do
    case "$arg" in
    --build) CARGO_BUILD=1 ;;
    --no-cleanup) NO_CLEANUP=1 ;;
    *)
        echo "unknown flag: $arg" >&2
        exit 2
        ;;
    esac
done

if ! command -v hyperfine >/dev/null; then
    echo "FATAL: hyperfine not on PATH (it is in the devShell — run 'nix develop' or 'direnv allow')" >&2
    exit 2
fi
if ! command -v jq >/dev/null; then
    echo "FATAL: jq not on PATH" >&2
    exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [ "$CARGO_BUILD" -eq 1 ]; then
    cargo build --release --quiet
fi
bin="$repo_root/target/release/repoctx"
if [ ! -x "$bin" ]; then
    echo "FATAL: $bin missing — run with --build or 'cargo build --release' first" >&2
    exit 2
fi

corpus="$(mktemp -d -t repoctx-bench-XXXXXX)"
cleanup() {
    if [ "$NO_CLEANUP" -eq 0 ]; then
        rm -rf "$corpus"
    else
        echo "corpus left at $corpus"
    fi
}
trap cleanup EXIT

echo "Synthesizing 5,000-file corpus at $corpus ..."
python3 - "$corpus" <<'PY'
import os, sys
root = sys.argv[1]
os.makedirs(os.path.join(root, ".git"))

EXTS = [".rs", ".go", ".ts", ".js", ".py", ".json", ".yaml", ".toml", ".md"]
DIRS = 50
PER_DIR = 100  # 50 * 100 = 5000

def body(ext, i):
    name = f"Item{i}"
    if ext == ".rs":
        return f"pub fn func_{i}() {{}}\npub struct {name};\n"
    if ext == ".go":
        return f"package x\nfunc Func{i}() {{}}\ntype {name} struct{{}}\n"
    if ext == ".ts":
        return f"export interface I{i} {{ x(): string }}\nexport abstract class A{i} {{ abstract y(): string }}\n"
    if ext == ".js":
        return f"class {name} {{ m{i}() {{}} }}\nfunction func_{i}() {{}}\n"
    if ext == ".py":
        return f"class {name}:\n    pass\n\ndef func_{i}():\n    pass\n"
    if ext == ".json":
        return f'{{"name_{i}": "x", "version_{i}": 1, "kind_{i}": "data"}}\n'
    if ext == ".yaml":
        return f"name_{i}: x\nversion_{i}: 1\nkind_{i}: data\n"
    if ext == ".toml":
        return f'name_{i} = "x"\n[pkg_{i}]\nver = "1"\n'
    if ext == ".md":
        return f"# Title {i}\n\nbody\n\n## Sub {i}\n"
    raise RuntimeError(ext)

count = 0
for d in range(DIRS):
    subdir = os.path.join(root, f"sub{d}")
    os.makedirs(subdir)
    for i in range(PER_DIR):
        ext = EXTS[(d + i) % len(EXTS)]
        with open(os.path.join(subdir, f"f{i}{ext}"), "w") as fh:
            fh.write(body(ext, count))
        count += 1
print(f"wrote {count} files")
PY

FAILED=0
declare -A MEANS

run_one() {
    local label="$1" budget_ms="$2" warmup="$3"
    shift 3
    echo
    echo "=== ${label}  (budget ${budget_ms} ms)"
    local json
    json="$(mktemp)"
    hyperfine --warmup "$warmup" --runs 5 --export-json "$json" --shell=none "$@" >/dev/null
    local mean_ms
    mean_ms="$(jq -r '.results[0].mean * 1000 | floor' "$json")"
    rm -f "$json"
    echo "    mean: ${mean_ms} ms"
    if [ "$mean_ms" -gt "$budget_ms" ]; then
        echo "    FAIL: exceeded budget ${budget_ms} ms"
        FAILED=$((FAILED + 1))
    else
        echo "    OK (${budget_ms} ms budget)"
    fi
    MEANS[$label]=$mean_ms
}

# 1. Cold index: tear down .repoctx then run. ≤ 10s for 5k files.
run_one cold_index 10000 0 \
    "sh -c 'rm -rf $corpus/.repoctx && $bin --repo $corpus --json --no-record index >/dev/null'"

# 2. No-op incremental index. ≤ 300 ms.
"$bin" --repo "$corpus" --json --no-record index >/dev/null
run_one noop_index 300 2 \
    "$bin --repo $corpus --json --no-record index"

# 3. Warm symbols query (common substring). ≤ 100 ms.
run_one warm_symbols 100 3 \
    "$bin --repo $corpus --json --no-record symbols func"

# 4. status --fast. ≤ 50 ms.
run_one status_fast 50 3 \
    "$bin --repo $corpus --json --no-record status --fast"

echo
echo "===================="
echo "  bench summary"
echo "===================="
printf "  cold index      : %s ms\n" "${MEANS[cold_index]}"
printf "  noop index      : %s ms\n" "${MEANS[noop_index]}"
printf "  warm symbols    : %s ms\n" "${MEANS[warm_symbols]}"
printf "  status --fast   : %s ms\n" "${MEANS[status_fast]}"
echo

if [ "$FAILED" -gt 0 ]; then
    echo "RESULT: $FAILED budget(s) exceeded"
    exit 1
fi
echo "RESULT: all budgets met"
