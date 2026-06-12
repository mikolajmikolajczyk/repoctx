# Commands

Everyday commands for this project. Keep this file **flat and copy-pasteable** — agents and humans both grep it.

## Build / run / test

```sh
cargo build                                       # debug build
cargo build --release                             # release build
cargo run -- <args>                               # run repoctx CLI
cargo test                                        # unit + integration tests
cargo test -p <crate>                             # scope to one workspace member
cargo test --test hook_e2e                        # hook CLI e2e suite alone
bash scripts/bench.sh                             # 5k-file synthetic perf bench
```

## Typecheck / lint / format

```sh
cargo check --all-targets                         # fast typecheck (no codegen)
cargo clippy --all-targets -- -D warnings         # lint, warnings fatal
cargo fmt                                         # format
cargo fmt --check                                 # CI-style format check
```

## Pre-commit

```sh
pre-commit install                                  # one-time, per clone
pre-commit run --all-files                          # run active hooks
pre-commit run --all-files --hook-stage manual      # run staged-as-manual hooks too
```

## Releasing

```sh
# 1. Bump workspace version in Cargo.toml + write CHANGELOG.md [<new>] block.
# 2. Pre-flight on a release branch:
cargo build --release
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
# 3. Commit + tag (GPG-signed) + push:
git commit -am "release: v<new>"
git push rad HEAD:refs/patches                    # land via radicle workflow
git tag -s v<new> -m "repoctx <new>"
git tag --verify v<new>                           # verify GPG sig
git push rad v<new>
git push origin v<new>                            # triggers .github/workflows/release.yml
```

GitHub Releases workflow builds + uploads 4-target archives + sha256 sidecars automatically.

## Hook (integrations)

```sh
cargo run -- hook list                            # enumerate agents
cargo run -- hook status                          # which dests exist
cargo run -- hook install <agent> --dry-run       # plan, write nothing
cargo run -- hook install claude --dir /tmp/proj  # content is embedded; no network
```

## Radicle

See [`../../.agents/skills/radicle/SKILL.md`](../../.agents/skills/radicle/SKILL.md) for the canonical CLI cheat-sheet. Most-used:

```sh
rad issue list --all
rad issue list --label state:in-progress
rad issue show <hex7>
rad issue open --title "<x>" --label "milestone:<m>" --label "priority:<p>"
rad issue label <hex7> -a state:in-progress
rad issue state --solved <hex7>
git push rad HEAD:refs/patches
```
