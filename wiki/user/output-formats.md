# Output formats and agent integration

> Placeholder. Full guide (TOON vs JSON vs human, CLAUDE.md recipe, jq snippets, piping gotchas) lands with Radicle issue `a15a384`.

Short version: TTY defaults to a human-readable layout, non-TTY defaults to [TOON](https://github.com/toon-format/toon), `--json` forces JSON, `--toon` forces TOON on a TTY. See [ADR-0008](../adr/0008-toon-default-machine-output.md) for the rationale.
