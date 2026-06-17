#!/usr/bin/env bash

# >>> repoctx (managed — edits here are overwritten) >>>
repoctx prime 2>/dev/null
# <<< repoctx (managed) <<<
# Claude Code SessionStart hook.
# Prints a quick orientation snapshot so the agent doesn't burn tokens
# rediscovering project state.

set -u

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" 2>/dev/null || exit 0

print_section() {
  printf '\n--- %s ---\n' "$1"
}

print_section "branch + last 5 commits"
git log --format="%h %s" -5 2>/dev/null || echo "(no git)"

print_section "in-progress issues (gh)"
if command -v gh >/dev/null 2>&1; then
  gh issue list --label state:in-progress --limit 10 2>/dev/null
  count=$(gh issue list --label state:in-progress --limit 100 2>/dev/null | wc -l)
  [ "${count:-0}" -eq 0 ] && echo "(none — nothing flagged state:in-progress)"
fi

print_section "milestone snapshot (open issues per milestone)"
if command -v gh >/dev/null 2>&1; then
  gh issue list --limit 100 --json labels 2>/dev/null \
    | grep -oE "milestone:[a-z0-9-]+" \
    | sort | uniq -c | sort -rn | head -10
fi

print_section "load order reminder"
cat <<'EOF'
1. AGENTS.md (root) → conventions + pointer table
2. wiki/agents/working-on-issues.md → if picking up an issue
3. gh issue view <N> --comments → recent comments on the active issue
4. Read only the wiki/agents/*.md files relevant to the task
EOF

exit 0
