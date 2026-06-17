# Commands

Everyday commands for this project. Keep this file **flat and copy-pasteable** — agents and humans both grep it.

## Build / run / test

```sh
cargo build                                       # debug build
cargo build --release                             # release build
cargo run -- <args>                               # run repoctx CLI
cargo test                                        # unit + integration tests
cargo test -p <crate>                             # scope to one workspace member
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
git tag -s v<new> -m "repoctx <new>"
git tag --verify v<new>                           # verify GPG sig
git push origin main --tags                        # GitHub; tag push triggers .github/workflows/release.yml
```

GitHub Releases workflow builds + uploads 4-target archives + sha256 sidecars automatically.

## Onboarding (integrations)

```sh
cargo run -- init --dry-run                        # plan the install, write nothing
cargo run -- init --agent claude                   # guidance + SessionStart prime hook
cargo run -- init --agent codex                    # guidance only (rules-only agent)
cargo run -- prime                                 # print the session-start digest
```

## GitHub (issues + code review)

Issue tracking, roadmap, and code review all live on GitHub — use `gh` (the GitHub CLI). Most-used:

```sh
gh issue list
gh issue list --label state:in-progress
gh issue view <N>
gh issue create --title "<x>" --label "milestone:<m>" --label "priority:<p>"
gh issue edit <N> --add-label state:in-progress
gh issue close <N>

gh pr create                                      # open a PR (code review on GitHub)
git push origin main                              # push code to GitHub
```
