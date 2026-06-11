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
    #[allow(dead_code)] // consumed when symbols command lands (0c56169)
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
/// 3. stdout is a TTY => Human.
/// 4. Otherwise => Toon (machine default per ADR-0008).
pub fn resolve(json: bool, toon: bool) -> Render {
    if json {
        Render::Json
    } else if toon {
        Render::Toon
    } else if io::stdout().is_terminal() {
        Render::Human
    } else {
        Render::Toon
    }
}

/// Human-side rendering. Implementations return the text to print
/// (renderer appends one trailing newline).
pub trait HumanRender {
    fn human(&self) -> String;
}

/// Generic list wrapper for commands that return many items. Same logical
/// shape in TOON and JSON: `{ "count": N, "items": [...] }`.
#[allow(dead_code)] // consumed when symbols command lands (0c56169)
#[derive(Debug, Clone, Serialize)]
pub struct List<T: Serialize> {
    pub count: usize,
    pub items: Vec<T>,
}

#[allow(dead_code)] // consumed when symbols command lands (0c56169)
impl<T: Serialize> List<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            count: items.len(),
            items,
        }
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
#[allow(dead_code)] // consumed when symbols command lands (0c56169)
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

    #[test]
    fn resolve_flags_override_tty() {
        assert_eq!(resolve(true, false), Render::Json);
        assert_eq!(resolve(false, true), Render::Toon);
    }
}
