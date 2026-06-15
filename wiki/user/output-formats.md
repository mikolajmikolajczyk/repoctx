# Output formats + AI agent integration

`repoctx` emits the same logical record in three encodings — human, [TOON](https://github.com/toon-format/toon), and JSON — so the same command can drive a terminal, a script, and an AI agent without `jq` gymnastics. The TOON-by-default policy and the rationale live in [ADR-0008](../adr/0008-toon-default-machine-output.md).

## Which format fires when

| Context | Default | Override |
|---|---|---|
| Terminal (stdout `isatty()`) | Human | `--toon` to force TOON, `--json` to force JSON |
| Pipe / non-TTY | TOON | `--json` to force JSON |
| Mutually exclusive | `--json` and `--toon` cannot be combined; clap rejects with an error. | — |

The same data, three encodings — same field names, same nesting, same exit codes.

## Side-by-side example

`repoctx symbols Render --limit 5` against this repo on 2026-06-11. Identical logical payload, three encodings:

### Human (TTY default)

```text
crates/repoctx/src/output.rs:49        HumanRender                                       interface
crates/repoctx/src/output.rs:10        Render                                            class
crates/repoctx/src/output_tests.rs:36  render                                            function
crates/repoctx/src/output_tests.rs:72  toon_renders_without_panic_and_ends_with_newline  function
```

### TOON (non-TTY default; `--toon` to force on a TTY)

```text
count: 4
items[4]:
  - name: HumanRender
    kind: interface
    location:
      path: crates/repoctx/src/output.rs
      start_line: 48
      start_column: 0
      end_line: 50
      end_column: 1
  - name: Render
    kind: class
    location:
      path: crates/repoctx/src/output.rs
      start_line: 9
      start_column: 0
      end_line: 13
      end_column: 1
```

### JSON (`--json`)

```json
{"count":4,"items":[{"name":"HumanRender","kind":"interface","location":{"path":"crates/repoctx/src/output.rs","start_line":48,"start_column":0,"end_line":50,"end_column":1}}, ...]}
```

## Token cost — why TOON

JSON pays for every comma, brace, and quoted key. TOON drops the structural noise and uses an indentation/colon shape that LLM tokenizers compress well. The savings dominate at scale — long lists of homogeneous records, the exact shape `repoctx symbols`/`search`/`outline`/`definition`/`context`/`callers`/`callees`/`callgraph` returns. For one-off small payloads (1–2 records, nested shapes) the two are roughly even; the policy still defaults to TOON for non-TTY callers because (a) bulk queries are common and (b) format flag toggling complicates agent prompts.

If your call site is parsing the result with `jq` or `serde_json`, pass `--json` — TOON's a different grammar.

## Stability promise

Field names and `kind` vocabulary are the public contract. Adding a field is non-breaking; renaming or removing one is breaking and ships with a CHANGELOG entry + (for serious shape changes) a deprecation window. See [`commands.md`](commands.md) for the current `kind` vocabulary (and the upstream-quirks table that surprises everyone).

## Agent integration

### Claude Code (CLAUDE.md snippet)

Drop this into the project's `CLAUDE.md` so the agent prefers `repoctx` over grep:

```markdown
## Code navigation

Use `repoctx` instead of grepping when answering structural questions
about this repo.

- `repoctx symbols <name>` — case-insensitive substring search. Add
  `--kind` (`function`/`class`/`section`/…) or `--lang` to narrow.
- `repoctx definition <name>` — exact-name lookup, definition kinds
  only. Prefer over `symbols` when you know the identifier.
- `repoctx outline <file>` — symbol tree for one file. Prefer over
  reading the whole file when you only need structure.
- `repoctx context <symbol>` — exact-name match + source window
  around each hit. Prefer over `definition` + a follow-up Read when
  you'd open the file anyway.
- `repoctx search <pattern>` — symbol defs + every textual match
  (comments/strings included), compressed. Use when non-symbol
  mentions matter; it's also what `rg <ident>` rewrites to.
- `repoctx callers <name>` / `callees <name>` / `callgraph <name>` —
  static call-graph edges; `callgraph` traverses (`--depth`,
  `--direction`).
- `repoctx status` — counts + staleness; cheap, run before deep work.
- `repoctx gain` — show the navigation tokens repoctx has saved.

Output defaults to TOON (token-efficient) for piped reads; pass
`--json` if you need to parse with jq/serde.
```

(Replace `<name>` with the agent's actual placeholder syntax if your harness needs it.)

### Generic script / jq

```sh
# All public functions in the codebase
repoctx --json symbols '' --kind function \
  | jq -r '.items[] | "\(.location.path):\(.location.start_line + 1)  \(.name)"'

# How many symbols are tagged in each language
repoctx --json status | jq -r '.per_language[] | "\(.language): \(.files) file(s)"'

# Exit-code-driven check: did any symbol containing 'TODO' get indexed?
if repoctx --json symbols TODO | jq -e '.count > 0' >/dev/null; then
  echo "Found TODO-tagged symbols"
fi
```

Empty results are `count: 0` with exit 0 — distinguish "nothing found" from "command failed" by checking the exit status, not by string-matching stderr.

### Pipeline gotcha — piped output is TOON unless `--json`

This is the single most common surprise:

```sh
# Wrong: jq can't parse TOON
repoctx symbols main | jq '.items'
# parse error

# Right
repoctx --json symbols main | jq '.items'
```

When in doubt, add `--json` to any command whose output flows into a JSON parser.

## See also

- [ADR-0008 — TOON as default machine output](../adr/0008-toon-default-machine-output.md)
- [TOON spec](https://github.com/toon-format/spec)
- [`gain.md`](gain.md) — how much navigation `repoctx` has actually saved your agent
