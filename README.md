# repoctx

AI-oriented repository intelligence CLI. Fast semantic code navigation using **Tree-sitter** and optional **LSP** backends, with **SQLite** as the source of truth for indexed metadata.

Built for coding agents and humans who need answers like _"where is symbol X defined?"_, _"what calls Y?"_, _"give me the surrounding context for line Z"_ — without spinning up an editor or paying full re-index cost on every query.

## Status

Pre-alpha. Scaffolding only. See Radicle issues for the active milestone (`rad issue list --label milestone:m0-foundation`).

## Getting started

Requires [Nix](https://nixos.org/download) (with flakes enabled) and [direnv](https://direnv.net/) for the recommended workflow.

```sh
git clone <repo-url> repoctx
cd repoctx
direnv allow                      # or: nix develop
cargo build
cargo run -- --help
```

Without Nix: install a recent stable Rust toolchain (see `flake.nix` for the pinned version) and run the same `cargo` commands.

Full install + first-query walkthrough: [`wiki/user/index.md`](wiki/user/index.md).

## Contributing

Canonical forge is **Radicle**. GitHub mirror exists for discoverability only.

```sh
rad clone <rid>                   # RID printed after first publish
rad issue list --all
git push rad HEAD:refs/patches    # submit a patch
```

GitHub PRs may not be monitored. Prefer Radicle issues/patches; otherwise open an issue describing what you'd like to send.

## License

LGPL-3.0-or-later — see [`LICENSE`](LICENSE).

Chosen to keep modifications to `repoctx` itself open while permitting integration into proprietary workflows.
