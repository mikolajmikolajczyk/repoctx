# Grammar crates: tree-sitter-toml-ng + tree-sitter-md, core pinned to 0.25.x

**Date:** 2026-06-11
**Decider:** Mikołaj Mikołajczyk
**Tags:** library-choice

## Context

ADR-0002 fixes the 9-language set but not the crates. The "obvious" names for TOML and Markdown are abandoned: `tree-sitter-toml` (0.20.0, 2022) and `tree-sitter-markdown` (0.7.1, 2021) require tree-sitter core ^0.20 and won't coexist with a modern core.

## Decision

Pin tree-sitter core 0.25.x with: tree-sitter-go 0.25.0, tree-sitter-rust 0.24.2, tree-sitter-typescript 0.23.2, tree-sitter-javascript 0.25.0, tree-sitter-python 0.25.0, tree-sitter-json 0.24.8, tree-sitter-yaml 0.7.2, **tree-sitter-toml-ng 0.7.0**, **tree-sitter-md 0.5.3 with default features only** (its `parser` feature pulls core 0.26). All depend on `tree-sitter-language ^0.1`, so they coexist with one core.

## Alternatives considered

- **tree-sitter-toml / tree-sitter-markdown** — abandoned, core ^0.20 conflict.
- **core 0.26** — would force `-md`'s newer line, but other grammars lag; 0.25 is the widest compatible set today.

## Trigger to revisit

Any grammar bump that breaks the compatible set, or core 0.26+ adoption across all 9 grammars.
