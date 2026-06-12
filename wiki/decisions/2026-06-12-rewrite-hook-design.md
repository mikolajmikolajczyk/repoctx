# Transparent rewrite hook — design

**Date**: 2026-06-12. **Issue**: `20ab48a`. **Epic**: `086b84b`. **Milestone**: v0.5.0.

## What

Pin the rewrite rule set + safety boundaries + interop strategy with
[rtk](https://github.com/rtk-ai/rtk) before code lands. Subsequent
children implement against this doc.

## Problem

Friends-of-the-project reported that AI coding agents progressively
forget to use `repoctx` after the SKILL.md is loaded — the guidance
fades as the conversation grows and the model falls back to `grep`
instincts. The fix is to intercept the agent's existing tool calls
transparently and route them through `repoctx` when it's the better
answer.

## Coexistence with rtk (Claude Code's hook execution model)

Verified against Claude Code 2.1.112's `cli.js`:

- **Multiple PreToolUse hooks under the same matcher run in
  parallel** via `Array.map` + `Promise.race`.
- **`updatedInput` has no precedence logic.** Whichever hook finishes
  last silently overwrites earlier results. Non-deterministic across
  runs.
- **`permissionDecision` DOES have a precedence ladder**: `deny >
  defer > ask > allow`. We don't use it — `deny` blocks the command,
  which is the opposite of what we want.
- No `priority` / `order` field exists on hook entries.

**Conclusion**: two independently-registered hooks cannot reliably
coexist on `updatedInput`. The only deterministic solution is **single
hook entry**.

## Design — chain dispatch with take-ownership install

### Runtime: `repoctx hook claude`

1. Read PreToolUse JSON from stdin.
2. Parse `tool_input.command`.
3. Try our semantic rewrite rules (below). If one matches, emit
   ```json
   {
     "hookSpecificOutput": {
       "hookEventName": "PreToolUse",
       "permissionDecision": "allow",
       "permissionDecisionReason": "repoctx: <orig> → <new>",
       "updatedInput": { "command": "<rewritten>" }
     }
   }
   ```
   and exit 0.
4. If no rule matched, for each `cmd` in
   `config.hook.chain_commands` (in order):
   - `exec` `cmd` with the same stdin.
   - Exit 0 with `updatedInput` → propagate that JSON, exit 0.
   - Exit 1 (silent passthrough) → try the next chain command.
   - Exit 2 (deny) → propagate verbatim, exit 2.
   - Any other failure → log to stderr, treat as passthrough.
5. All chains passthrough → exit 1.

### Install: `repoctx hook install claude`

When patching `.claude/settings.json`:

1. Read the file (or treat as empty if missing).
2. Scan `hooks.PreToolUse[]` entries whose `matcher == "Bash"`.
3. For each `hooks[]` entry under those matchers:
   - Save its `command` into `config.hook.chain_commands` (preserve
     order; append to any existing list — re-installs don't lose
     state).
   - Skip entries already pointing at `repoctx hook claude` (no
     self-chain).
4. Remove the old `matcher == "Bash"` PreToolUse entries.
5. Insert a single fresh entry:
   ```json
   {
     "matcher": "Bash",
     "hooks": [{"type": "command", "command": "repoctx hook claude"}]
   }
   ```
6. Print a removal recipe that includes the saved
   `hook.chain_commands` so the user can restore the original setup
   by hand.

### Drift: `repoctx hook doctor`

If a sibling tool's installer (rtk update, manual edit, another
agent-tooling installer) re-adds a Bash matcher entry after our
takeover, the parallel-race problem returns. `repoctx hook doctor`:

1. Re-runs steps 1-5 of the install pipeline.
2. Reports what it absorbed.
3. Exits 0 even if nothing changed (idempotent).

Documentation recommends running it after any other PreToolUse-touching
install. Future enhancement: a cheap per-invocation drift check that
emits a stderr warning when ownership has been lost (deferred — adds
per-call overhead).

## Rewrite rule set (initial)

Conservative. Only rewrite when the agent's intent is clearly
"find a symbol by name". Otherwise: passthrough → chain.

| Agent pattern | Rewritten to | Notes |
|---|---|---|
| `rg <ident>` (single token, no flags) | `repoctx symbols <ident> --json` | `ident` matches `[A-Za-z_][A-Za-z0-9_]*` |
| `rg -l <ident>` | `repoctx symbols <ident> --json` + agent extracts paths via downstream tooling | Optional first cut; may defer |
| `rg "fn <ident>"` / `rg "function <ident>"` / `rg "class <ident>"` / `rg "struct <ident>"` | `repoctx definition <ident> --json` | The classic "where is X defined" pattern |
| `grep -r <ident> .` | `repoctx symbols <ident> --json` | Same shape as `rg <ident>` |
| `grep -rn "fn <ident>"` (same family for class/struct) | `repoctx definition <ident> --json` | Same intent as the rg variant |

**Hard no-rewrite cases**:

- Regex patterns (`.*`, `^`, `$`, character classes, `\b`, `(`,
  alternation `|`, etc.).
- Multiple identifiers (`rg "foo bar"`).
- File-path patterns (`rg foo src/lib.rs`).
- Counting (`-c`).
- Output suppression (`-q`, `-l` for "just file names" is the only
  exception).
- Binary mode (`-a`, `--binary`).
- Anything with shell metacharacters (backticks, `$(...)`, `>`, `&&`,
  `||`, `;`, `|`).
- `--lang` / `-t` filters — repoctx supports `--lang` but the slug
  vocabularies differ between rg and repoctx (`rust` vs `rs`, etc.);
  defer mapping.

Anything in the "hard no" set → passthrough. The chain handles it.

## Advisory-aware rewriting

The coverage advisory layer from v0.3.0 (`Language::coverage()`) is
the trump card. Before rewriting, check whether the matched files in
the workspace are dominated by `Partial`-coverage languages
(JSON/YAML/TOML). If so, skip the rewrite even on a syntactically
matching pattern — the agent gets better answers from rtk's
formatted grep than from `repoctx`'s top-keys-only response.

Specifically:

1. Compute the rewrite's repoctx invocation.
2. Spawn it with `--json` (already part of the rewrite).
3. Parse the result. If the `advisory` field is present AND mentions
   `partial coverage`, AND `count` is below a threshold (e.g. < 3),
   then bail — passthrough to chain.

Cheaper alternative: don't pre-run repoctx. Just emit the rewrite
and trust the agent to follow the advisory if it fires. Less robust
but no double-execution cost.

**Decision for v0.5.0**: emit the rewrite, trust the advisory. Defer
the pre-execution dry-run check until we have data showing the
naive path actually hurts.

## `hook.rewrite` config integration

From v0.4.0:

- `hook.rewrite = "auto"` (default): execute the design above.
- `hook.rewrite = "off"`: skip step 3 (no semantic rewrite ever);
  go straight to chain dispatch. repoctx becomes a pure proxy in
  front of rtk. Useful for users who want chain behavior without
  repoctx semantic intercept.
- `hook.rewrite = "force"`: relax the conservative parser; rewrite
  more aggressively. Debug/test only — documented as not for
  production.

## Telemetry / gain integration

Every rewrite (and every chain-handled passthrough) writes a `usage`
row tagged `command = "hook-rewrite"`. `repoctx gain top --all`
shows hook activity alongside the rest. `count` reflects the
rewrites the hook fired (not the tokens saved on the rewritten
command itself — that's covered when the rewritten command runs).

## Removal recipe template

```text
Installed claude. To remove:
  - in .claude/settings.json, restore the original PreToolUse → Bash
    entries by adding back these commands under hooks[]:
        <each command from hook.chain_commands, one per line>
  - remove the entry {"type": "command", "command": "repoctx hook claude"}
  - run: repoctx config unset hook.chain_commands
```

## Acceptance

- [ ] Per the rule set: a literal `rg parse_config` → routed through
      `repoctx definition` family.
- [ ] A regex-containing rg (`rg "^fn.*\("`) passes through cleanly
      (no rewrite emitted).
- [ ] With rtk in `chain_commands`, an unmatched `grep "TODO"` is
      rewritten to rtk's formatted version (chain delegation works).
- [ ] `hook.rewrite = off` → every command falls through to chain
      regardless of pattern.
- [ ] `hook doctor` re-takes ownership idempotently.
- [ ] Removal recipe accurately restores rtk's original entry.

## Out of scope

- Codex / opencode rewrite hooks. They have different hook protocols.
  Defer to a follow-up epic.
- Multi-matcher support (Read, Edit, etc). Bash only for v0.5.0.
- Pre-execution dry-run check for advisory bailout. Defer until data
  warrants it.
- Agent-side override (`--no-rewrite` per-command). Use `hook.rewrite
  = off` instead.

## Sources

- Claude Code 2.1.112 `cli.js`. PreToolUse executor `E0`, hook merger
  with precedence ladder.
- rtk inventory: `src/cmds/{system,git,...}/`. Generic non-destructive
  output compression model.
- Coverage advisory layer from v0.3.0 (`4604399`).
- Config layer from v0.4.0 (`2c96964`, `hook.rewrite` already plumbed).
