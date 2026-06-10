# Deferred

Things **deliberately not implemented**. If something seems missing and is listed here, don't add it unprompted — there's a reason. Each entry: what, why deferred, when to revisit.

## Format

```markdown
### <Feature / behavior>

- **Why deferred:** <one paragraph>
- **Revisit when:** <trigger condition>
- **Tracked in:** <radicle issue hex7, if any>
```

## Entries

<TBD: filled as decisions accumulate. Examples of typical entries:

- error retry / backoff machinery (premature without observed flakiness)
- plugin sandbox (trust model not yet defined)
- i18n (single-locale project for now)
- telemetry / analytics (privacy decision pending)
- DB layer (in-memory is enough for current scope)
>
