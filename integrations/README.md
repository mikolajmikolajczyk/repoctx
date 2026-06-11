# `repoctx` integrations

Per-agent install machinery for `repoctx hook`. Subdirectories carry the
manifest + content fetched at install time.

| Agent | Subdir |
|---|---|
| Claude Code | `claude/` |
| Codex | `codex/` |
| opencode | `opencode/` |
| Shared fragments | `shared/` |

The schema for `manifest.toml` lives in `crates/integrations/src/manifest.rs`.
