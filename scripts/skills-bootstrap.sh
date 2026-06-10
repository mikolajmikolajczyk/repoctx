#!/usr/bin/env bash
# skills-bootstrap — fetch llm_skills.sh, install skills declared in .agents/skillfile,
# and symlink them into .claude/skills/ for Claude Code auto-discovery.
#
# Idempotent: safe to re-run. Pulls the latest llm_skills.sh every time.
# Skills resolve from llm_skills.sh's persistent cache (~/.local/share/llm_skills/repos/),
# not from a tempdir — symlinks survive after this script exits.

set -euo pipefail

LLM_SKILLS_RAW="https://raw.githubusercontent.com/mikolajmikolajczyk/llm_skills/master/llm_skills.sh"

if [[ ! -f .agents/skillfile ]]; then
  echo "error: .agents/skillfile not found (run from project root)" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "• fetching llm_skills.sh"
curl -fsSL "$LLM_SKILLS_RAW" -o "$tmp/llm_skills.sh"
chmod +x "$tmp/llm_skills.sh"

echo "• syncing skills from .agents/skillfile"
"$tmp/llm_skills.sh" sync .

# Mirror into .claude/skills/ so Claude Code can auto-trigger skills
# (Claude scans .claude/skills/, not .agents/skills/).
if [[ -d .agents/skills ]]; then
  mkdir -p .claude/skills
  for skill_dir in .agents/skills/*/; do
    [[ -d "$skill_dir" ]] || continue
    name=$(basename "$skill_dir")
    ln -sfn "../../.agents/skills/$name" ".claude/skills/$name"
    echo "• .claude/skills/$name → .agents/skills/$name"
  done
fi

echo "✓ skills installed"
