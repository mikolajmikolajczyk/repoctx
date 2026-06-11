# Dev setup

Toolchain and local-environment setup. Stack-specific toolchain pinning (Nix flake, mise, asdf, rustup, nvm, uv, ...) is added at bootstrap.

## direnv (optional but recommended)

`.envrc` ships in the repo. Allow it once per clone:

```sh
direnv allow
```

If you use a Nix flake (add at bootstrap), `.envrc` typically contains `use flake` and direnv auto-loads the devShell on `cd`.

## Pre-commit

```sh
pre-commit install
```

Hooks come from `.pre-commit-config.yaml`. The template ships generic hooks (whitespace, EOF, YAML/JSON checks, markdownlint, shellcheck, gitleaks, GPG UID guard). Add language-specific hooks (formatter, linter, typechecker) at bootstrap or later.

Run all hooks on demand:

```sh
pre-commit run --all-files
pre-commit run --all-files --hook-stage manual   # includes manual-staged hooks
```

## GPG signing

The `gpg-uid-guard` pre-commit hook (always active) refuses to sign when `user.email` has no matching UID on `user.signingkey`. Fix path if it fails:

```sh
git config user.email <your-email>
git config user.signingkey <key-id>
# or attach a matching UID to the key with `gpg --edit-key <key>`
```

## Stack-specific toolchain

**This project uses Nix flake + direnv.** `flake.nix` pins the Rust toolchain and CLI tooling; `.envrc` contains `use flake` so direnv auto-loads the devShell on `cd`.

Enter the shell:

```sh
nix develop          # explicit
direnv allow         # one-time; thereafter automatic on cd
```

What the devShell provides:

- `rustc` + `cargo` (stable, pinned via `nixpkgs`)
- `rustfmt`, `clippy`, `rust-analyzer`
- `pkg-config`, `sqlite` (system lib for `rusqlite`/`libsqlite3-sys`)
- `pre-commit`, `shellcheck`, `gitleaks`, `markdownlint-cli`

Non-Nix fallback (not actively supported): install a recent stable Rust via `rustup`, plus `sqlite` and `pkg-config` from your distro. You'll need to install the pre-commit tooling separately and reproducibility is on you.

## CI + GitHub mirror

Canonical forge is Radicle. A read-only mirror lives at <https://github.com/mikolajmikolajczyk/repoctx> for CI and discoverability only — patches/issues are not monitored there. CI lives in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml): plain `cargo` (no nix) across `ubuntu-latest`, `macos-latest`, `windows-latest`, running `fmt --check`, `build`, `test`, `clippy -D warnings`. To sync: `git push origin main` after a merge.
