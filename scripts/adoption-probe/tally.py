#!/usr/bin/env python3
"""Classify the tool calls in a Claude Code stream-json transcript.

Reads JSONL on stdin (one Claude `--output-format stream-json` line per row),
counts how the agent chose to navigate: did it reach for `repoctx`, or for
grep/find/cat and the native Read/Grep/Glob tools?

Emits one JSON object: {repoctx, grep, find, read, native_search, other,
used_repoctx}. The probe's whole question is whether priming flips `used_repoctx`
from false (grep loop) to true.
"""
import json
import re
import sys

GREP = re.compile(r"\b(rg|grep|egrep|fgrep|ag|ack)\b")
FIND = re.compile(r"\bfind\b")
READISH = re.compile(r"\b(cat|head|tail|sed|awk|less|more)\b")


def classify_bash(cmd: str) -> str:
    # A compound `cd repo; <tool>` still counts by the real tool.
    if "repoctx " in cmd or cmd.strip().endswith("repoctx"):
        return "repoctx"
    if GREP.search(cmd):
        return "grep"
    if FIND.search(cmd):
        return "find"
    if READISH.search(cmd):
        return "read"
    return "other"


def main() -> None:
    counts = {"repoctx": 0, "grep": 0, "find": 0, "read": 0, "native_search": 0, "other": 0}
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        msg = ev.get("message") if isinstance(ev, dict) else None
        if not isinstance(msg, dict):
            continue
        for block in msg.get("content", []) or []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = block.get("name", "")
            inp = block.get("input", {}) or {}
            if name == "Bash":
                counts[classify_bash(str(inp.get("command", "")))] += 1
            elif name in ("Grep", "Glob"):
                counts["native_search"] += 1
            elif name == "Read":
                counts["read"] += 1
    counts["used_repoctx"] = counts["repoctx"] > 0
    print(json.dumps(counts))


if __name__ == "__main__":
    main()
