//! Output layer — three renderers (human / TOON / JSON) over the same
//! `serde::Serialize` types. ADR-0008.

use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Render {
    Human,
    Toon,
    Json,
}

impl Render {
    /// CLI-flag name for the format (used for telemetry / gain recording).
    pub fn name(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Toon => "toon",
            Self::Json => "json",
        }
    }
}

/// Resolve the render format once at program start.
///
/// Precedence:
/// 1. `--json` => Json.
/// 2. `--toon` => Toon.
/// 3. `output.default` config (when not `auto`) overrides TTY detection.
/// 4. stdout is a TTY => Human.
/// 5. Otherwise => Toon (machine default per ADR-0008).
pub fn resolve(json: bool, toon: bool, default: crate::config::OutputDefault) -> Render {
    use crate::config::OutputDefault;
    if json {
        Render::Json
    } else if toon {
        Render::Toon
    } else {
        match default {
            OutputDefault::Human => Render::Human,
            OutputDefault::Toon => Render::Toon,
            OutputDefault::Json => Render::Json,
            OutputDefault::Auto => {
                if io::stdout().is_terminal() {
                    Render::Human
                } else {
                    Render::Toon
                }
            }
        }
    }
}

/// Human-side rendering. Implementations return the text to print
/// (renderer appends one trailing newline).
pub trait HumanRender {
    fn human(&self) -> String;
}

/// Generic list wrapper for commands that return many items. Same logical
/// shape in TOON and JSON: `{ "count": N, "items": [...], "advisory"?: "…" }`.
///
/// `advisory` is omitted in the happy path. When present (`Some`) it
/// tells the agent the current backend is underserving this query and
/// suggests a fallback (typically a `ripgrep` invocation).
#[derive(Debug, Clone, Serialize)]
pub struct List<T: Serialize> {
    pub count: usize,
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

impl<T: Serialize> List<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            count: items.len(),
            items,
            advisory: None,
        }
    }

    pub fn with_advisory(mut self, advisory: Option<String>) -> Self {
        self.advisory = advisory;
        self
    }
}

/// Render `value` to `out`, appending one trailing newline.
pub fn emit_to<W, T>(out: &mut W, value: &T, render: Render) -> Result<()>
where
    W: Write,
    T: Serialize + HumanRender,
{
    match render {
        Render::Human => {
            out.write_all(value.human().as_bytes())?;
            out.write_all(b"\n")?;
        }
        Render::Json => {
            serde_json::to_writer(&mut *out, value).context("json encode")?;
            out.write_all(b"\n")?;
        }
        Render::Toon => {
            let opts = toon_format::EncodeOptions::default();
            let s = toon_format::encode(value, &opts).context("toon encode")?;
            out.write_all(s.as_bytes())?;
            if !s.ends_with('\n') {
                out.write_all(b"\n")?;
            }
        }
    }
    Ok(())
}

/// Convenience: render to `stdout`.
pub fn emit<T>(value: &T, render: Render) -> Result<()>
where
    T: Serialize + HumanRender,
{
    let mut out = io::stdout().lock();
    emit_to(&mut out, value, render)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::OutputDefault;

    #[test]
    fn resolve_flags_override_tty() {
        assert_eq!(resolve(true, false, OutputDefault::Auto), Render::Json);
        assert_eq!(resolve(false, true, OutputDefault::Auto), Render::Toon);
    }

    #[test]
    fn resolve_config_default_overrides_tty_detection() {
        assert_eq!(resolve(false, false, OutputDefault::Json), Render::Json);
        assert_eq!(resolve(false, false, OutputDefault::Toon), Render::Toon);
        assert_eq!(resolve(false, false, OutputDefault::Human), Render::Human);
    }

    #[test]
    fn resolve_cli_flag_beats_config_default() {
        assert_eq!(resolve(true, false, OutputDefault::Toon), Render::Json);
        assert_eq!(resolve(false, true, OutputDefault::Json), Render::Toon);
    }

    // ── Format snapshot tests (ADR-0008 contract) ────────────────────

    use repoctx_backend::{Location, Symbol, SymbolKind};

    fn fixture() -> List<Symbol> {
        List::new(vec![
            Symbol {
                name: "main".into(),
                kind: SymbolKind::Function,
                location: Location {
                    path: "src/main.rs".into(),
                    start_line: 0,
                    start_column: 0,
                    end_line: 0,
                    end_column: 4,
                },
            },
            Symbol {
                name: "MyType".into(),
                kind: SymbolKind::Class,
                location: Location {
                    path: "src/lib.rs".into(),
                    start_line: 9,
                    start_column: 0,
                    end_line: 9,
                    end_column: 6,
                },
            },
        ])
    }

    fn render_fixture(r: Render) -> String {
        let mut buf = Vec::new();
        emit_to(&mut buf, &fixture(), r).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn json_shape_is_compact_with_trailing_newline() {
        assert_eq!(
            render_fixture(Render::Json),
            concat!(
                r#"{"count":2,"items":["#,
                r#"{"name":"main","kind":"function","location":{"path":"src/main.rs","start_line":0,"start_column":0,"end_line":0,"end_column":4}},"#,
                r#"{"name":"MyType","kind":"class","location":{"path":"src/lib.rs","start_line":9,"start_column":0,"end_line":9,"end_column":6}}"#,
                "]}\n",
            )
        );
    }

    #[test]
    fn human_is_aligned_columns_one_based_line() {
        assert_eq!(
            render_fixture(Render::Human),
            "src/main.rs:1  main    function\nsrc/lib.rs:10  MyType  class\n",
        );
    }

    #[test]
    fn human_empty_list() {
        let mut buf = Vec::new();
        emit_to(&mut buf, &List::<Symbol>::new(vec![]), Render::Human).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "no symbols\n");
    }

    #[test]
    fn toon_renders_without_panic_and_ends_with_newline() {
        let s = render_fixture(Render::Toon);
        assert!(
            s.ends_with('\n'),
            "toon output should end with newline: {s:?}"
        );
        assert!(s.contains("main"), "toon should mention 'main': {s:?}");
        assert!(s.contains("MyType"), "toon should mention 'MyType': {s:?}");
    }
}
