# repoctx wiki

Project knowledge tree. Two audiences:

- [`agents/`](agents/) — for coding agents (Claude Code, Cursor, Aider, ...). Sized for load-on-demand. Entry: [`../AGENTS.md`](../AGENTS.md) pointer table.
- [`user/`](user/) — for humans (end users, contributors). End-user docs, tutorials, screenshots.

Cross-cutting:

- [`adr/`](adr/) — Architecture Decision Records. Lasting, app-shaping decisions.
- [`decisions/`](decisions/) — Cross-cutting decision log. Smaller than ADRs, bigger than issue comments.

Skills (radicle, radboard, ...) live under `.agents/skills/` — fetched by `scripts/skills-bootstrap.sh` from [llm_skills](https://github.com/mikolajmikolajczyk/llm_skills). Not in wiki because they're not docs, they're agent capabilities.
