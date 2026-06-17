# Commands reference

Commands: `index`, `symbols`, `search`, `outline`, `definition`, `context`, `callers`, `callees`, `callgraph`, `deadcode`, `impact`, `cycles`, `deps`, `rdeps`, `boundary`, `import-cycles`, `modules`, `overview`, `communities`, `report`, `export`, `prime`, `changed`, `status`, `languages`, `config`, `init`, `gain`. (`callers`/`callees`/`callgraph` are the static call graph, ADR-0010; `deps`/`rdeps`/`boundary` are the import / dependency graph, ADR-0011; `search` is the textually-complete search, epic `f4cb992`.)

## Global flags

| Flag | Effect |
|---|---|
| `--repo <PATH>` | Treat `<PATH>` as the search start (default: cwd). Repo root = nearest ancestor (incl. itself) containing `.git`, else the given dir. |
| `--json` | Force JSON output even on a TTY. Mutually exclusive with `--toon`. |
| `--toon` | Force [TOON](https://github.com/toon-format/toon) output even on a TTY. |
| `--no-record` | Skip gain analytics recording for this invocation. |
| `--record-query` | Persist the query string in the usage row (off by default). |
| `-v` / `-vv` | Verbosity: `-v` = info, `-vv` = debug. `RUST_LOG` overrides. |

Output format defaults: TTY → human; non-TTY → TOON; `--json` always JSON. See [`output-formats.md`](output-formats.md) and [ADR-0008](../adr/0008-toon-default-machine-output.md).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. **An empty result set is a success** — list commands print `count: 0` and exit 0. |
| `1` | Any error. Error message goes to stderr; machine stdout stays empty. |

Errors most users will see:

- Missing-index errors don't exist for users — read commands always build the DB if needed and run an incremental reindex (one short stderr line when work happened, otherwise silent) before answering.
- `index is locked by another repoctx process — retry` — two writers raced for longer than the 5 s busy timeout.
- `index is corrupted — delete .repoctx/ and re-run 'repoctx index'` — the DB file is non-SQLite or corrupted.
- `index was created by a newer repoctx — upgrade repoctx or delete .repoctx/` — schema version is ahead of this binary.

## `repoctx index`

Walk the repository, parse changed files in parallel, write through a single sequential SQLite writer, prune absent files, print a one-line summary.

| Flag | Effect |
|---|---|
| `--force` | Reparse every file, even those with an unchanged `(mtime, size)` tuple. |

What is **always** skipped: `.git/`, `.repoctx/`, files matched by `.gitignore`/`.git/info/exclude`/the user's global gitignore, files larger than **2 MiB**, files that don't decode as UTF-8. Skipped files get a single `WARN` line on stderr each.

Example (machine summary via `--json`):

```sh
repoctx index --json
```

```json
{"indexed":81,"unchanged":0,"removed":0,"duration_ms":69}
```

## `repoctx symbols <query>`

Case-insensitive substring search across every indexed symbol.

| Flag | Effect |
|---|---|
| `--kind <kind>` | Restrict to one kind (see the table below). |
| `--lang <slug>` | Restrict to one language slug (`rust`, `go`, `python`, `typescript`, `tsx`, `javascript`, `c`, `cpp`, `java`, `csharp`, `ruby`, `php`, `lua`, `kotlin`, `swift`, `bash`, `markdown`, `json`, `yaml`, `toml`). |
| `--limit <N>` | Cap results at `N` (default `50`; `0` = unlimited). |

Result ordering is deterministic: `name COLLATE NOCASE ASC`, then `file_path ASC`, then `start_line ASC`.

### Kind vocabulary

`repoctx` reuses each Tree-sitter grammar's upstream `tags.scm` as-is (ADR-0002). The full enum surfaced by `--kind` is:

```text
function method class struct enum interface trait module
constant type variable field macro section key other
```

**Some upstream quirks worth knowing:**

| Source code | Reported kind | Why |
|---|---|---|
| Rust `struct`, `enum`, `union`, `type` | `class` | Upstream Rust `tags.scm` collapses them all to `@definition.class`. |
| Rust `trait` | `interface` | Same upstream choice. |
| Go `type X struct {}` / `type Y interface {}` | `type` | Upstream Go `tags.scm` uses `@definition.type`. |
| TypeScript / TSX `class`, `function`, arrow `const X = () => ...`, `type`, `enum` | tagged (vendored Aider `tags.scm`, Apache-2.0) | repoctx ships a richer query than upstream `tree-sitter-typescript`. Covers plain class, plain function, arrow-function assigned to identifier, type aliases, enums, interfaces, and abstract classes. |
| Markdown headings | `section` | Custom query — ATX (`#`) and setext (`===`/`---`) headings. |
| JSON/YAML/TOML top-level keys | `key` | Custom query — root-level keys only; nested keys not surfaced. |

Filter and tweak as needed: `--kind class --lang rust` to walk every Rust struct/enum/type alias, `--kind section --lang markdown` for a table-of-contents view, etc.

## `repoctx outline <file>`

Document-symbol view of a single file. Human mode prints an indented containment tree (a method's range sits inside its impl's range, so the method nests). Machine mode is a flat `{count, items}` list with full 0-based ranges, so downstream tools don't have to reconstruct the tree.

Path argument accepts both forms:

- Repo-relative: `repoctx outline crates/repoctx/src/main.rs`
- Absolute: `repoctx outline /full/path/to/main.rs`

Both are canonicalized and re-anchored against the repo root before lookup. A path outside the repo bails with `path is outside repo: …`.

If the file isn't in the index, you get:

```text
crates/foo/bar.rs is not in the index — file may be new, ignored, oversized
(>2 MiB), non-UTF-8, or in an unsupported language. Run `repoctx index` to refresh.
```

Exit 1. Those four causes are the entire set — no other reason exists for a file to be missing once `index` succeeds.

```sh
repoctx outline --json crates/repoctx/src/outline_cmd.rs
```

```json
{"count":7,"items":[{"name":"OutlineReport","kind":"class","location":{"path":"crates/repoctx/src/outline_cmd.rs","start_line":15,"start_column":0,"end_line":20,"end_column":1}}, …]}
```

## `repoctx definition <name>`

Exact-name (case-sensitive) lookup. **Contrast with `symbols`**: `symbols foo` matches any name containing `foo` (substring, case-insensitive); `definition foo` matches only names that are exactly `foo`. Use `definition` when you know the identifier and want the canonical site; use `symbols` to explore.

| Flag | Effect |
|---|---|
| `--lang <slug>` | Restrict to one language slug. |
| `--limit <N>` | Cap results at `N` (default `50`). |

The kind filter is fixed — **only** these kinds can be a "definition":

```text
function method class interface type module macro constant
```

Variables, fields, sections, keys, and `other` are excluded so a search for `path` doesn't drown in struct-field hits. Rust `struct`/`enum`/`trait` reach this set via upstream `tags.scm` mapping (see the kind quirks table under `symbols`).

Multiple hits are normal (think `run` defined in many modules) — the command lists all of them; the agent picks. Zero hits is a clean exit 0 with `count: 0`.

```sh
repoctx definition --json main --lang rust
```

```json
{"count":1,"items":[{"name":"main","kind":"function","location":{"path":"crates/repoctx/src/main.rs","start_line":161,"start_column":0,"end_line":238,"end_column":1}}]}
```

## `repoctx context <symbol>`

The flagship agent query: "where is X defined AND what does it look like?". One call returns every exact-name match plus a window of surrounding source.

| Flag | Effect |
|---|---|
| `--context <C>` | Lines of leading and trailing context. Default `5`. `0` returns body only. |
| `--limit <N>` | Maximum number of matches. Default `3`. |

Matching: exact name (case-sensitive), any kind. Ranking when there are more hits than `--limit`: shorter file path first, then `(start_line, start_column)`. So a top-level `main` in `src/main.rs` ranks above a vendored copy in `vendor/lib/src/main.rs`.

For each match, the source window is read **from disk** (not the DB) so you get current bytes. The window is `start_line - C .. end_line + C`, clamped to file bounds (top-of-file → empty `before`, bottom-of-file → empty `after`).

Each item carries a `stale` flag. Effectively always `false` in normal use: `context` auto-reindexes changed files before answering, so the indexed `location` matches what's on disk. The flag survives in the schema for edge cases (concurrent file edits, files modified between the reindex and the source read).

If the file was deleted since indexing, the match is dropped with a `WARN` line on stderr and remaining matches still print.

Machine shape per match:

```json
{
  "symbol": "main",
  "kind": "function",
  "location": {"path": "crates/repoctx/src/main.rs",
               "start_line": 161, "start_column": 0,
               "end_line": 238, "end_column": 1},
  "before": "…",
  "body": "fn main() -> Result<()> {\n    let cli = Cli::parse();\n    …\n}",
  "after": "…",
  "stale": false
}
```

Wrapper: `{count, items}`. Human mode prints `# path:line  name  kind` per match, an optional `(stale …)` line, then a numbered listing.

```sh
repoctx context resolve_window --context 2 --limit 1
```

```text
# crates/repoctx/src/main.rs:241  resolve_window  function
  239  }
  240
  241  fn resolve_window(since: Option<&str>, all: bool) -> Result<gain_cmd::Window> {
  242      if all {
  243          return Ok(gain_cmd::Window::All);
  244      }
  …
```

## `repoctx search <pattern>`

Textually-complete search with **provenance**: one flat `results` stream
where every item is tagged with how much to trust it —

- `structural` — tree-sitter confirmed a symbol definition here (name, kind,
  range known). Highest confidence. Each structural symbol carries its own
  `callers`/`callees` (see below).
- `reference` — a call site of the queried name (from the call graph). Medium.
- `textual` — substring matched, AST didn't confirm it (comments, strings,
  a call to a *different* symbol). Grep-level.

So the agent can tell confirmed symbols from noise at a glance, and — the
thing grep can't do — see who calls the symbol and what it calls, in one query.

**Callers/callees are grouped by how the name resolves *within the indexed
scope*** (not by repo boundary), to keep the signal dense:

- `internal` — resolves to exactly one indexed symbol. The valuable case
  ("calls your function X, here"). Always expanded with location.
- `ambiguous` — resolves to several indexed symbols. Collapsed to a per-name
  count (`{name, count}`), e.g. `new: 4 internal candidates`.
- `external_count` — calls whose definition isn't in the indexed scope
  (stdlib / third-party / builtin / uncovered-language file). Collapsed to a
  count, so a dozen `format`/`Some`/`Ok` calls don't bury the internal ones.

`--all-callees` expands the collapsed categories (`external` name list +
ambiguous `candidates`). No stop-list — external-ness is just index absence.

| Flag | Effect |
|---|---|
| `--lang <slug>` | Restrict textual matches to a language (maps to rg `--type`). |
| `--limit <N>` | Cap files returned. Default `50` (also capped at 40 internally). |
| `--all-callees` | Expand `external` names + ambiguous `candidates` (default: counts only). |

Caps keep token cost low: ≤40 files, ≤8 matches/file, ≤50 call edges, lines
truncated at 200 chars. Truncation is flagged (`truncated` + `advisory`). If
ripgrep isn't on PATH, you get the structural results only + an advisory.
Lines are 0-based in machine output (human mode prints 1-based).

```json
{"pattern":"parse_config",
 "results":[
   {"source":"structural","path":"src/config.rs","line":41,"name":"parse_config","kind":"function","end_line":58,
    "callers":{"internal":[{"name":"main","path":"src/main.rs","line":4,"kind":"function"}]},
    "callees":{"internal":[{"name":"validate","path":"src/config.rs","line":70,"kind":"function"}],
               "ambiguous":[{"name":"new","count":4}], "external_count":7}},
   {"source":"reference","path":"src/main.rs","line":5,"text":"    parse_config();"},
   {"source":"textual","path":"README.md","line":11,"text":"run parse_config first"}
 ]}
```

Use `search` when you might care about non-symbol mentions (a value in a
comment/string, a config key). Use `symbols`/`definition` when you only want
the structural answer.

## `repoctx callers <name>` / `repoctx callees <name>`

Direct call-graph edges (static, Tree-sitter; ADR-0010).

- `callers <name>` — every call site whose callee is named `<name>` (who calls it).
- `callees <name>` — every call made from within a symbol named `<name>` (what it calls).

| Flag | Effect |
|---|---|
| `--limit <N>` | Maximum number of edges. Default `50`. `0` = unlimited. |

Each edge carries the resolved caller symbol, the callee (resolved symbol or `null` when external/unresolved), the call-site location, `resolution` (`syntactic` now; `semantic` once an LSP backend lands), and `ambiguous` (the callee name resolves to more than one symbol).

```sh
repoctx callers parse_config
```

```json
{"count":1,"items":[{
  "caller":{"name":"main","kind":"function","location":{"path":"src/main.rs","start_line":3,"start_column":0,"end_line":9,"end_column":1}},
  "callee_name":"parse_config",
  "callee":{"name":"parse_config","kind":"function","location":{"path":"src/config.rs","start_line":40,"start_column":0,"end_line":52,"end_column":1}},
  "site":{"path":"src/main.rs","start_line":5,"start_column":4,"end_line":5,"end_column":4},
  "resolution":"syntactic","ambiguous":false}]}
```

## `repoctx callgraph <name>`

Transitive call graph from `<name>` — breadth-first over edges, cycle-safe, depth-bounded.

| Flag | Effect |
|---|---|
| `--depth <N>` | Traversal depth. Default `3`. `1` = direct edges only. |
| `--direction <up\|down\|both>` | `down` = callees (what it calls), `up` = callers (who calls it), `both`. Default `down`. |

Each item is a call edge tagged with `depth` (1 = direct) and `direction`. A safety cap (2000 edges) truncates pathological fan-out, surfaced via the advisory.

```sh
repoctx callgraph handle_request --depth 2 --direction down
```

### Accuracy caveats (read before trusting the graph)

The call graph is **name-based and approximate — the same accuracy class as `definition`**, not LSP-grade:

- **No receiver-type disambiguation.** `a.foo()` and `b.foo()` both resolve to *every* symbol named `foo`. Such edges are flagged `ambiguous: true`.
- **External/unresolved callees are listed with `callee: null`** (stdlib, third-party, or dynamically dispatched). The name is shown; the location is unknown.
- **Dynamic dispatch, function pointers, and higher-order calls are invisible.**
- **Cross-language edges are out of scope.**
- **Languages:** the core 8 (Rust, Python, JavaScript, TypeScript, Go, C, C++, Java). Other indexed languages return no edges until a follow-up adds their call queries.

When edges are ambiguous or unresolved, the command emits an `advisory` pointing at `rg` as the fallback. Treat the output as a strong hint, not a proof.

**`--resolved-only`** (on `callers`/`callees`/`callgraph`/`impact`): drop ambiguous + external edges, keeping only those that resolve to a single in-repo symbol — the trustworthy core of the graph. Without it, resolved edges are sorted first so they lead. `overview` hotspots apply the same idea automatically (single-definition names only, host/builtin method names like `get`/`set`/`push` excluded).

## `repoctx deadcode` / `impact` / `cycles`

Tier-1 analyses over the call graph (no new indexing — pure queries over the `calls` table). All inherit the **name-based** accuracy class (ADR-0010): dynamic dispatch, trait objects, FFI, and callers outside the indexed scope are invisible, so output is a **candidate list to verify, not proof**. Each carries an advisory.

- **`repoctx deadcode [--lang L] [--limit N]`** — function/method symbols with zero incoming call edges. Excludes entry points (`main`), `constructor`, test files, `.d.ts` declarations, minified/generated files, and **exported/public symbols where the language has visibility extraction** (Go = capitalized; JS/TS = inline `export`; more incrementally, #10). Something grep can't do. Caveat: in languages without visibility yet (`unknown`), public API still shows up; dynamically-called functions (trait dispatch, registry/spread) always do — verify before deleting.
- **`repoctx impact <name> [--depth N]`** — blast radius: everything that transitively *calls* `name` ("if I change this, what breaks"). Frames `callgraph <name> --direction up`.
- **`repoctx cycles [--limit N]`** — recursion / mutual-recursion cycles in the call graph. In-repo edges only; cycles rotated to a canonical start + deduped. Very large graphs (20k+ edges) are skipped with an advisory.

```sh
repoctx deadcode --lang rust
repoctx impact parse_config --depth 2
repoctx cycles
```

## `repoctx deps <file>` / `repoctx rdeps <module>`

The import / dependency graph (ADR-0011). `deps` lists the module specifiers a file imports; `rdeps` lists the files that import a module.

- `deps <file>` — `<file>` is repo-relative or absolute. Items are `{file, module, line, resolution}`, one per import site, ordered by source position.
- `rdeps <module>` — matches any import specifier **containing** `<module>` as a substring, so `rdeps storage-idb` finds every importer of `@adapters/storage-idb`. Items share the same shape; `file` is the importer.

```sh
repoctx deps src/ui/AssetPanel.tsx
repoctx rdeps @adapters/storage-idb      # who imports the storage adapter?
repoctx rdeps storage-idb                # same, by substring
```

### Accuracy caveats

String-based and approximate, mirroring the call graph:

- **The raw specifier is stored as written** (quotes/angle-brackets stripped). Aliased/package specifiers (`@adapters/x`, `react`) match exactly; relative specifiers (`./x`, `../y`) are verbatim, so `rdeps` by bare name is most useful for aliases/packages. `deps` by file is exact regardless.
- **No specifier→file resolution.** tsconfig paths, `node_modules`, and crate layout are not resolved yet (deferred to a future resolver writing `semantic` edges into the same table).
- **`rdeps` substring matching can over-match** (`util` matches `./my-util`). The exact `module` field in `--json` lets you disambiguate.
- **Languages:** the core 8 (Rust `use`/`extern crate`, Python `import`/`from`, JS/TS/TSX ESM `import`/`export … from`, Go imports, C/C++ `#include`, Java `import`). Other indexed languages return no edges yet.

Empty results carry an `advisory` pointing at `rg` as the fallback.

## `repoctx boundary --from <path> --to <module>`

Layering / boundary check over the import graph: list every file whose path contains `--from` that imports a specifier containing `--to`. Answers "does layer A import layer B?" structurally — no regex over import lines, no eslint-boundary comments.

| Flag | Effect |
|---|---|
| `--from <substr>` | Importer path substring — the layer doing the importing (e.g. `src/ui`). |
| `--to <substr>` | Imported specifier substring — the target layer (e.g. `@adapters`). |
| `--forbid` | CI gate: exit 1 if any crossing exists (else exit 0). |

```sh
# Does the UI layer import the storage adapter directly?
repoctx boundary --from src/ui --to @adapters/storage-idb

# Fail CI if @plugins reaches into @adapters:
repoctx boundary --from src/plugins --to @adapters --forbid
```

Output is the crossing edges (`{file, module, line}`). A crossing is counted when an import from `--from` **resolves** to a file path containing `--to` — relative imports **and tsconfig path aliases** (#8), so `--to src/adapters` catches `@adapters/*` imports (you can also pass the alias directly: `--to @adapters`). `count: 0` is honest: the advisory reports how many bare/unresolved imports (node_modules / unmapped aliases) couldn't be checked, rather than a blind "clean."

## `repoctx import-cycles` / `modules`

Graph analyses over the import graph (petgraph). To get file→file edges, **relative** specifiers (`./x`, `../y`) **and tsconfig path aliases** (`@adapters/*` → `src/adapters/*`, from any `tsconfig*.json`/`jsconfig.json` at the repo root) are resolved against the indexed file set; bare/package specifiers (`react`) and non-TS module syntax (Rust/Python/Go) stay `external`. Best for JS/TS.

- **`repoctx import-cycles [--limit N]`** — circular imports (strongly-connected groups of files that import each other, directly or transitively).
- **`repoctx modules`** — the resolved import topology: `{files, edges, external_edges, cyclic, order, dependencies}`. `order` is a dependency-first build order (toposort), empty when the graph is cyclic. `dependencies` lists the resolved `from → to` edges (capped at 500).

```sh
repoctx import-cycles
repoctx modules
```

## `repoctx overview`

Repo architecture in one call — the "dropped into an unfamiliar repo" command. Composes what the index + call graph already hold (no new extraction):

- `files` / `symbols` / `code_symbols` totals (doc/config = `symbols − code_symbols`) + per-`languages` symbol counts
- `modules` — per-directory `{dir, files, code_symbols, symbols, bytes}`, **ranked by code symbols** (top 30). Markdown headings + config keys count as doc/config, not code, so `wiki/`/`.github/`/`docs/` no longer top the list (#9-D)
- `entry_points` — `main` functions/methods + JS/TS web-app bootstraps (`main.tsx`, `index.tsx`, …)
- `hotspots` — most-called symbols (incoming call-edge count). Receiver-aware (a `.set()` method call binds only to a repo `method`, never a free `function`) + single-callable-def, so the ranking is centrality, not name popularity (name-based per ADR-0010; #9)

```sh
repoctx overview
```

Public API surface (exported symbols per module) is **not** included yet — it needs per-language export extraction (#8); the advisory notes this.

## `repoctx communities`

Cluster the call graph into subsystems — the "where are the seams in this repo" command. Runs single-level Louvain modularity optimization over the **resolved** call graph (unambiguous edges, callees resolving to a single callable definition), then:

- `count` — number of **subsystems**: Louvain clusters with **≥ `analysis.subsystem_min_size` members** (default 5). This is the shared definition `report` and `export` also use, so all three report the same number.
- `communities` — each subsystem's `{label, size, members}`, ranked by size (display capped at top 30, 15 members each). `label` = the cluster's highest-degree member.
- `god_nodes` — highest-degree symbols overall (top 15): the cross-cutting hubs that touch many subsystems.

Pure topology — name-based (ADR-0010), no embeddings or LLM. The Louvain partition is **deterministic** (reproducible across runs). Clustering ambiguous fan-out yields noise, so the input is resolved-only by construction. Empty/non-core-8-language repos get an advisory instead of clusters.

```sh
repoctx communities
```

## `repoctx report`

One-page architecture report, **generated deterministically from graph topology — no LLM, no network**. Composes the resolved call graph into:

- **God nodes** — highest-degree symbols (cross-cutting hubs).
- **Subsystems** — `communities` (#14) clusters with ≥ `analysis.subsystem_min_size` members (same count `communities`/`export` report), labeled by representative member.
- **Cross-cluster bridges** — call edges whose endpoints sit in different subsystems, ranked by combined endpoint degree. The coupling worth scrutinizing.
- **Entry points** — `main`-like symbols (same heuristic as `overview`).
- **Suggested questions** — templated from structure (top god node, largest subsystem, top bridges). Orientation prompts, **not** findings.

Human output *is* the report markdown — pipe it (`repoctx report > REPORT.md`) or use `--out`:

| Flag | Effect |
|---|---|
| `--out <path>` | Write the markdown report to a file (e.g. `REPORT.md`). Always writes markdown regardless of `--json`/`--toon`. |

`--json` / `--toon` emit the structured form (`{nodes, edges, communities_count, god_nodes, communities, bridges, entry_points, questions, advisory}`).

```sh
repoctx report                  # markdown to stdout
repoctx report --out REPORT.md  # write the file
repoctx report --json           # structured data
```

Name-based (ADR-0010); the same single-callable-def resolution as `communities`. An opt-in `--llm` prose-narration layer is deferred.

## `repoctx export`

Export an **interactive, self-contained HTML graph** of the call graph — **no CDN, no build step, no server**. The graph is embedded as JSON and laid out by a small hand-rolled force simulation in vanilla JS, so the file opens offline in any browser.

- Nodes = symbols, **sized by degree**. Only real **subsystems** (clusters ≥ `analysis.subsystem_min_size`, the same count as `report`) get distinct colors; the tiny-cluster tail + the ambiguous/unclustered layer render **grey** — so subsystems read as colored islands.
- Edges = call edges, **styled by ambiguity** — dashed amber = name-ambiguous, solid green = resolved. The differentiator: repoctx knows which edges are uncertain and the viz shows it.
- **Layer toggle** — hide the ambiguous/unclustered layer for a clean subsystem view, or keep it for the full graph-with-uncertainty.
- Subtitle reports both sides honestly: `N subsystems · S symbols (R resolved + A ambiguous/builtin) · E edges (Re resolved + Ae ambiguous)`.
- Interaction: drag nodes, scroll to zoom, drag background to pan, search highlights matching symbols, legend toggles subsystems on/off.

| Flag | Effect |
|---|---|
| `--out <path>` | Write the HTML to a file (e.g. `graph.html`). Without it, the HTML is printed to stdout. |

```sh
repoctx export --out graph.html   # then open graph.html in a browser
```

External call targets (no in-repo definition) are dropped so the graph stays about this repo. Name-based (ADR-0010).

## `repoctx prime`

A compact, token-budgeted (~600 token) **session-start orientation digest** — meant to be injected into an agent's context so it begins primed to use repoctx instead of blind `grep`/`cat`. Deterministic markdown to stdout: headline (files / code symbols / top languages), top subsystems (#14, 3 members each), highest-degree hubs, entry points, and a one-block `repoctx` skill pointer. The full call graph is referenced by command (`repoctx export`), never inlined.

`repoctx init` (for Claude) registers this as a **SessionStart** hook automatically, so you normally don't run it by hand — the digest is injected into the agent's context at session start. It never cold-indexes — if the repo isn't indexed yet it emits a one-line nudge and exits (keeping session start fast); otherwise it refreshes incrementally.

```sh
repoctx prime          # print the digest (what the SessionStart hook injects)
```

## `repoctx changed [--since REF]`

Change-aware blast radius — pairs with code review. Diffs the working tree against a git ref, finds the symbols overlapping the changed lines, and walks their transitive callers.

| Flag | Effect |
|---|---|
| `--since <ref>` | Git ref to diff against (working tree vs ref). Default `HEAD` (uncommitted changes); use `main` for a whole PR. |

Output: `{since, files_changed, changed:[{name,kind,path,line}], impacted:[{name,kind,path,line,depth}]}` — `impacted` is the transitive callers (blast radius), `depth` = hops from a changed symbol.

```sh
repoctx changed                 # uncommitted changes + what they break
repoctx changed --since main    # the whole branch's blast radius
```

Tracked files only (untracked aren't in `git diff`). Blast radius is name-based (ADR-0010) — verify; capped at 500 impacted symbols.

## `repoctx status`

Index health + per-language counts + optional staleness.

| Flag | Effect |
|---|---|
| `--fast` | Skip the staleness stat-walk (counts only). |

Output fields:

| Field | Meaning |
|---|---|
| `schema_version` | DB schema version (currently `9`). |
| `files` | Total indexed files. |
| `symbols` | Total symbols across all files. |
| `db_size_bytes` | On-disk size of `.repoctx/index.db`. |
| `per_language` | List of `{ language, files }` rows, alphabetical. |
| `staleness.changed` | Files whose `(mtime, size)` tuple no longer matches the index. |
| `staleness.new` | Files that exist on disk but aren't in the index yet. |
| `staleness.deleted` | Files that are in the index but no longer on disk. |

`--fast` drops the entire `staleness` block.

## `repoctx languages`

Print the per-language coverage matrix repoctx uses to advise agents on when to fall back to `ripgrep`. No arguments, no flags.

```sh
repoctx languages
```

```text
rust        full     struct/enum/union/type → class, trait → interface (upstream tags.scm)
go          full     func / method / type (struct/interface) (upstream tags.scm)
python      full     def / class (upstream tags.scm)
typescript  full     interface, class, function (incl. arrow), method, type, enum (vendored Aider tags.scm)
tsx         full     same coverage as TypeScript (vendored Aider tags.scm)
javascript  full     class, function (incl. arrow), method (upstream tags.scm)
markdown    full     ATX (#) and setext headings (custom query)
toml        partial  root pairs + [table] + [[array]] headers; keys inside tables are not surfaced
json        partial  top-level keys only; nested keys are not surfaced
yaml        partial  top-level keys of each document; nested keys are not surfaced
```

`coverage` is `full` or `partial`. Read commands attach an `advisory` field to their machine output when the query targets a `partial`-coverage language (or the workspace contains files in one and the query returned zero hits). The advisory text suggests a concrete `rg -n` fallback.

## Coverage advisory on read commands

`outline`, `definition`, `context`, and `symbols` may include an `advisory` field in their machine output. Always omitted in the happy path; present when:

- The target file's language is `partial` (`outline` over a YAML/JSON/TOML file).
- `--lang <slug>` was supplied and that slug is `partial`.
- `count == 0` and the workspace has at least one file in a `partial` language.

Human render appends a final `advisory: <text>` line. Machine renders include `"advisory": "..."`.

Agents should treat a non-null `advisory` as a hint to also run the suggested `rg` command and merge the results, rather than trusting `count: 0` as authoritative.

## `repoctx config`

Per-repo settings table (lives in `.repoctx/index.db`). Four
subcommands. Full reference + key schema + env-var naming + precedence
rules: [`config.md`](config.md).

| Subcommand | Effect |
|---|---|
| `repoctx config show` | Every effective key + its current value + source (`cli` / `env` / `settings` / `default`). |
| `repoctx config get <key>` | One value, with its source. |
| `repoctx config set <key> <value>` | Validate + write. Rejects unknown keys and out-of-range values. |
| `repoctx config unset <key>` | Delete row; built-in default applies again. |

Precedence (highest wins): CLI flag → environment variable → settings
row → built-in default. Keys today: `gain.no_record`, `gain.record_query`,
`output.default`, `index.nested_keys`, `analysis.subsystem_min_size`.

`analysis.subsystem_min_size` (default `5`, env
`REPOCTX_ANALYSIS_SUBSYSTEM_MIN_SIZE`): the minimum Louvain-cluster size
that counts as a "subsystem" in `communities`/`report`/`export`. One
shared definition so the three commands report the same count. Raise it
for fewer, larger subsystems; lower it (min `2`) for more.

## `repoctx init`

The single onboarding command. Installs the agent guidance files and,
for Claude, wires a **SessionStart** hook that runs `repoctx prime` so
the orientation digest is injected into the agent's context at session
start. Full reference: [`init.md`](init.md).

| Invocation | Effect |
|---|---|
| `repoctx init` | Project-scope install (guidance files + Claude SessionStart prime hook in `.claude/settings.json`). |
| `repoctx init -g` | User-global install (`~/.claude/`). |
| `repoctx init --agent <name>` | Pick the agent (`claude`, `codex`, `opencode`; default `claude`). |
| `repoctx init [--yes] [--force] [--dry-run]` | Skip prompts / override a refused install / plan-only. |
| `repoctx init --uninstall [-g]` | Remove the SessionStart hook + guidance (inverse of install). |

Per-agent guidance files are embedded in the binary — install works offline and always matches your installed version.

## `repoctx gain`

Surface the navigation cost the agent avoided. `gain` defaults to the **last 30 days**; the subcommand `gain top` ranks per command.

| Flag | Effect |
|---|---|
| `--since <window>` | Override the window. `7d`, `2h`, `30m`, `120s`. |
| `--all` | Drop the window — all-time totals. |
| `--history [N]` | Swap the summary for the N most recent rows (default `20`). |

### `repoctx gain top`

| Flag | Effect |
|---|---|
| `--by saved` | (default) Rank by absolute `estimated_savings`. Tiebreak on command name. |
| `--by ratio` | Rank by `reduction` percentage instead. |
| `--since`/`--all` | Same semantics as on `gain`. |

`gain` invocations are **not** themselves recorded. Empty usage in the window is a success: zeros, exit 0.

Full philosophy + privacy stance: [`gain.md`](gain.md).
