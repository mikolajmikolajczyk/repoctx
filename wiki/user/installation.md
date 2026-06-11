# Installation

`repoctx` is a single Rust binary, distributed as source today (no pre-built downloads — see [the M0 release issue `bc9da7c`](https://github.com/mikolajmikolajczyk/repoctx/issues)). Two install paths:

## Nix flake (recommended)

Pinned toolchain + SQLite headers come from the flake. Requires Nix with flakes enabled.

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
nix develop              # one-shot devShell (or `direnv allow` for auto-load)
cargo build --release
./target/release/repoctx --help
```

To install `repoctx` into `~/.cargo/bin` so it's on `PATH` everywhere:

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

## No binary releases yet

Pre-1.0. Once the v0.1.0 release ships ([Radicle issue `bc9da7c`](https://github.com/mikolajmikolajczyk/repoctx/issues)), this page will gain `nix profile install`, `cargo install repoctx` (crates.io decision pending), and platform binaries from GitHub Releases.
