# repoctx

AI-oriented repository intelligence CLI. **Tree-sitter** parses, **SQLite** stores, queries answer in milliseconds. Output defaults to [TOON](https://github.com/toon-format/toon) so the LLM on the other end of the pipe pays the fewest tokens for the same answer; `--json` for scripts.

## What works (M0)

- `repoctx index` — incremental walk + parse + persist; mtime-based invalidation; `--force` reparses everything.
- `repoctx symbols <query>` — case-insensitive substring search; `--kind`, `--lang`, `--limit` filters; deterministic ordering.
- `repoctx status` — counts, per-language breakdown, freshness (`{changed, new, deleted}`).
- `repoctx gain` — surface the navigation tokens repoctx has saved.
- Three output formats over one set of typed records (human / TOON / JSON).
- 9 languages out of the box: Go, Rust, TypeScript, TSX, JavaScript, Python, JSON, YAML, TOML, Markdown.
- CI green on Linux + macOS + Windows.

Bench baseline on a 5,000-file synthetic corpus: cold index 318 ms, no-op incremental 50 ms, warm `symbols` query 3 ms.

## Quickstart

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
nix develop                          # pinned toolchain + SQLite + bench tooling
cargo install --path crates/repoctx  # or: cargo build --release
cd /path/to/your/own/repo
repoctx index                         # one-time
repoctx symbols UserService           # query
```

```text
$ repoctx symbols Render --limit 3
count: 3
items[3]:
  - name: HumanRender
    kind: interface
    location:
      path: crates/repoctx/src/output.rs
      start_line: 48
      ...
```

Pipe-friendly TOON by default; pass `--json` for jq:

```sh
repoctx --json symbols main | jq '.items[].location.path'
```

Full walk-through (install, status, gain, output formats, agent integration): [`wiki/user/`](wiki/user/index.md).

## Contributing

Canonical forge is **Radicle**. GitHub mirror exists for CI and discoverability only — patches and issues there aren't monitored.

```sh
rad clone rad:z3ZAf4PfKZnuurn2YNz3t7cTLLUgB
cd repoctx
rad issue list --all
git push rad HEAD:refs/patches    # submit a patch
```

If Radicle isn't your thing, open a GitHub issue describing what you'd like to send and we'll figure it out.

## License

LGPL-3.0-or-later — see [`LICENSE`](LICENSE).

Chosen to keep modifications to `repoctx` itself open while permitting integration into proprietary workflows.
