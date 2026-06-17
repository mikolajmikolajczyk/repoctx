#!/usr/bin/env python3
"""Aggregate adoption-probe transcripts (out/<arm>-<task>-r<rep>.jsonl) into a
per-arm table: did the agent choose repoctx, or grep/find/cat/native-search?

Usage: summarize.py <out-dir>
"""
import glob
import json
import os
import re
import sys
from collections import defaultdict

GREP = re.compile(r"\b(rg|grep|egrep|fgrep|ag|ack)\b")
FIND = re.compile(r"\bfind\b")
READISH = re.compile(r"\b(cat|head|tail|sed|awk|less|more)\b")


def classify_bash(cmd: str) -> str:
    if "repoctx " in cmd or cmd.strip().endswith("repoctx"):
        return "repoctx"
    if GREP.search(cmd):
        return "grep"
    if FIND.search(cmd):
        return "find"
    if READISH.search(cmd):
        return "read"
    return "other"


def tally(path: str) -> dict:
    c = {"repoctx": 0, "grep": 0, "find": 0, "read": 0, "native_search": 0, "other": 0}
    with open(path) as fh:
        for line in fh:
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = ev.get("message") if isinstance(ev, dict) else None
            if not isinstance(msg, dict):
                continue
            for b in msg.get("content", []) or []:
                if not isinstance(b, dict) or b.get("type") != "tool_use":
                    continue
                n = b.get("name", "")
                if n == "Bash":
                    c[classify_bash(str((b.get("input") or {}).get("command", "")))] += 1
                elif n in ("Grep", "Glob"):
                    c["native_search"] += 1
                elif n == "Read":
                    c["read"] += 1
    return c


def main() -> None:
    out = sys.argv[1] if len(sys.argv) > 1 else "out"
    arms = defaultdict(list)
    for path in sorted(glob.glob(os.path.join(out, "*.jsonl"))):
        arm = os.path.basename(path).split("-", 1)[0]
        arms[arm].append(tally(path))

    print(f"\n{'arm':<10} {'runs':>4} {'used_rpx':>9} {'rpx/run':>8} "
          f"{'search/run':>11} {'grep':>5} {'find':>5} {'read+nat':>9}")
    print("-" * 66)
    for arm in ("bare", "guidance", "primed"):
        rows = arms.get(arm, [])
        if not rows:
            continue
        n = len(rows)
        used = sum(1 for r in rows if r["repoctx"] > 0)
        rpx = sum(r["repoctx"] for r in rows)
        grep = sum(r["grep"] for r in rows)
        find = sum(r["find"] for r in rows)
        rd = sum(r["read"] + r["native_search"] for r in rows)
        search = grep + find + rd
        print(f"{arm:<10} {n:>4} {f'{used}/{n}':>9} {rpx/n:>8.1f} "
              f"{search/n:>11.1f} {grep:>5} {find:>5} {rd:>9}")
    print("\nused_rpx = runs where the agent ran repoctx at least once.")
    print("Bet validated if used_rpx + rpx/run rise and search/run falls across "
          "bare -> guidance -> primed.")


if __name__ == "__main__":
    main()
