# Installation

Four install paths. Pick the one that fits.

## Pre-built binaries (fastest)

Every `v*` tag publishes archives for four targets at <https://github.com/mikolajmikolajczyk/repoctx/releases>.

| Target | Asset |
|---|---|
| Linux x86_64 | `repoctx-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `repoctx-<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `repoctx-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `repoctx-<version>-x86_64-pc-windows-msvc.zip` |

Each archive carries a `.sha256` sidecar. Verify before unpacking, then drop `repoctx` (or `repoctx.exe`) anywhere on `PATH`.

```sh
VERSION=0.5.1
TARGET=x86_64-unknown-linux-gnu
curl -LO https://github.com/mikolajmikolajczyk/repoctx/releases/download/v${VERSION}/repoctx-${VERSION}-${TARGET}.tar.gz
curl -LO https://github.com/mikolajmikolajczyk/repoctx/releases/download/v${VERSION}/repoctx-${VERSION}-${TARGET}.tar.gz.sha256
shasum -a 256 -c repoctx-${VERSION}-${TARGET}.tar.gz.sha256
tar xzf repoctx-${VERSION}-${TARGET}.tar.gz
sudo mv repoctx-${VERSION}-${TARGET}/repoctx /usr/local/bin/
repoctx --version
```

Windows (PowerShell):

```powershell
$Version = "0.5.1"
$Target  = "x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/mikolajmikolajczyk/repoctx/releases/download/v$Version/repoctx-$Version-$Target.zip" -OutFile "repoctx.zip"
Invoke-WebRequest "https://github.com/mikolajmikolajczyk/repoctx/releases/download/v$Version/repoctx-$Version-$Target.zip.sha256" -OutFile "repoctx.zip.sha256"
# Compare hash manually against the sidecar
(Get-FileHash repoctx.zip -Algorithm SHA256).Hash.ToLower()
Get-Content repoctx.zip.sha256
Expand-Archive repoctx.zip -DestinationPath .
# Move repoctx.exe somewhere on PATH (e.g. into a directory you've added to $env:Path)
```

The archive also contains `README.md`, `LICENSE`, and `CHANGELOG.md` for the matching version.

## Nix flake (reproducible)

The flake exposes both a `devShell` (pinned toolchain + SQLite + bench tooling) and a `packages.default` that builds the release binary.

One-shot run:

```sh
nix run github:mikolajmikolajczyk/repoctx -- --help
```

Install into your Nix profile:

```sh
nix profile install github:mikolajmikolajczyk/repoctx
repoctx --help
```

Local clone + dev shell:

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
nix develop                  # or `direnv allow` for auto-load
cargo build --release
./target/release/repoctx --help
```

To put a development build on `PATH` system-wide:

```sh
nix develop --command cargo install --path crates/repoctx
```

## Cargo (from source, no Nix)

You need:

- A stable Rust toolchain (the flake currently uses **rustc 1.95** — older toolchains may work but aren't tested; check `flake.nix` for the pinned baseline).
- A C compiler — needed for the Tree-sitter grammars. Linux: `gcc` from your distro. macOS: `xcode-select --install`. Windows: Visual Studio Build Tools (the `x86_64-pc-windows-msvc` target).

SQLite is bundled via the `rusqlite` crate's `bundled` feature — no system SQLite required on any platform.

Pin to a release:

```sh
cargo install --git https://github.com/mikolajmikolajczyk/repoctx --tag v0.5.1
```

Or from a clone:

```sh
git clone https://github.com/mikolajmikolajczyk/repoctx
cd repoctx
cargo install --path crates/repoctx
repoctx --help
```

## crates.io

Not yet — publishing is deferred until the API stabilizes. Track [CHANGELOG.md](../../CHANGELOG.md) for the current version.

## Verifying the install

```sh
repoctx --version
repoctx --help
```

You should see `repoctx 0.5.1` (or newer) plus the `index`, `symbols`, `outline`, `definition`, `context`, `status`, `languages`, `config`, `hook`, and `gain` subcommands. Then head to [`quickstart.md`](quickstart.md).
