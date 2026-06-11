# Installation

`repoctx` is a single Rust binary, distributed as source. Pick the path that fits:

## Nix flake (recommended)

The flake exposes both a `devShell` (pinned toolchain + SQLite + bench tooling) and a `packages.default` that builds the release binary. Requires Nix with flakes enabled.

One-shot run without installing anything:

```sh
nix run github:mikolajmikolajczyk/repoctx -- --help
```

Install into your Nix profile:

```sh
nix profile install github:mikolajmikolajczyk/repoctx
repoctx --help
```

Local clone + build (useful for development):

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
nix develop              # pinned dev environment (or `direnv allow` for auto-load)
cargo build --release
./target/release/repoctx --help
```

To put a development build on `PATH` system-wide:

```sh
nix develop --command cargo install --path crates/repoctx
```

## Plain Cargo (without Nix)

You need:

- A stable Rust toolchain (the flake currently uses **rustc 1.95** — older toolchains may work but aren't tested; check `flake.nix` for the pinned baseline)
- System SQLite + `pkg-config` headers (Ubuntu/Debian: `apt install libsqlite3-dev pkg-config`; macOS Homebrew: `brew install sqlite pkg-config`; Windows: `rusqlite`'s bundled feature is already enabled, so no extra step)

Then:

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
cargo install --path crates/repoctx
repoctx --help
```

## Verifying

After install:

```sh
repoctx --version
repoctx --help
```

You should see the version line plus the `index`, `symbols`, `status`, and `gain` subcommands. Once that's working, head to [`quickstart.md`](quickstart.md).

## Pre-built binaries + crates.io

Not yet. crates.io publishing is deferred until the API stabilizes (the binary install paths above are the supported ones for now), and platform binaries on GitHub Releases will land alongside the first tagged release. Track [CHANGELOG.md](../../CHANGELOG.md) for the current version.
