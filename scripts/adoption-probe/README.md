# adoption-probe — does priming change tool choice?

agent-bench measures token *savings if repoctx is used*. This probe measures
the prior question: **does an agent actually choose `repoctx` over grep/cat?**
— the bet behind session-start priming (decision 2026-06-16-adoption-via-priming).

## How

`run.sh <repo> [N]` runs each task in [`tasks.txt`](tasks.txt) headless
(`claude -p --output-format stream-json`) under two conditions and tallies the
tool calls the agent *chose* (the transcript records the model's command before
any PreToolUse rewrite, so it's true intent):

- **control** — `repoctx init --uninstall`: no SessionStart hook.
- **primed** — `repoctx init`: SessionStart hook injects `repoctx prime`.

[`tally.py`](tally.py) classifies each Bash command (`repoctx` vs
grep/find/cat) plus the native `Read`/`Grep`/`Glob` tools, and reports whether
`repoctx` was used at all.

```sh
scripts/adoption-probe/run.sh ~/src/madside        # all tasks
scripts/adoption-probe/run.sh ~/src/madside 2      # first 2 (cheap trial)
```

Real `claude -p` runs — **costs tokens**. Read-only nav tasks; runs with
`--dangerously-skip-permissions` so tools execute headless. Transcripts land in
`out/` (gitignored).

## Confound: the `bare` baseline

`control` only removes the SessionStart hook — the committed **guidance**
(`CLAUDE.md` block + `.claude/skills/repoctx/SKILL.md`) is still present, so the
agent may already choose `repoctx` from that alone. `control` vs `primed` thus
measures the prime's *marginal lift over guidance*, not over nothing.

For the full picture run three arms (the third by hand for now):

1. **bare** — a repo with NO repoctx guidance, NO skill, NO hook → the true
   grep baseline. (Strip/restore `CLAUDE.md`'s repoctx block + the skill dir, or
   use a clean clone.)
2. **guidance** — guidance + skill, no hook (= this harness's `control`).
3. **primed** — + SessionStart hook (= this harness's `primed`).

`bare → guidance` shows guidance's lift; `guidance → primed` shows priming's.

## Reading the result

The bet is validated if `used_repoctx` rises across arms and grep/read calls
fall — especially on the harder, exploratory tasks where the grep reflex is
strongest. If `guidance` already saturates `used_repoctx`, the SessionStart hook
may be redundant for direct nav (its value would be orientation on *unfamiliar*
repos, not command choice). Use enough repeats to see past LLM nondeterminism.
