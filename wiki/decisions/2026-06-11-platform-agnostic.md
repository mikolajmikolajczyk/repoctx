# Platform-agnostic from the start: Linux, macOS, Windows first-class

**Date:** 2026-06-11
**Decider:** Mikołaj Mikołajczyk
**Tags:** process, platform

## Context

The CLI's primary callers are AI coding agents, which run on all three desktop platforms. Porting a unix-assuming codebase later costs more than holding the line from commit one.

## Decision

M0/M1 code is platform-agnostic: no `std::os::unix` APIs; DB-stored paths are `/`-separated and cross the fs boundary only via a single helper pair in `store` (`to_db_path`/`from_db_path`); universal-newline handling where source lines are sliced; portable `Metadata::modified()` for mtime (granularity caveat recorded in ADR-0006). CI matrix (ubuntu/macos/windows) is the primary enforcement gate; `scripts/platform-check.sh` is a fast secondary gate run by both CI (Linux/macOS) and the local pre-commit hook, forbidding `std::os::unix`, `cfg(unix)`, `cfg(target_os = ...)`, `MAIN_SEPARATOR`, and scattered `'\\' -> '/'` munging. M2's daemon transport is abstracted: unix socket on unix, named pipe on Windows (noted in ADR-0005).

## Alternatives considered

- **Linux-only M0, port later** — cheaper now, but unix assumptions metastasize (paths, sockets, mtime accessors); port cost grows with every issue.
- **Linux+macOS, Windows best-effort** — Windows is where the path/newline bugs live; "best-effort" means "broken".

## Trigger to revisit

A dependency that cannot be made portable and has no substitute.
