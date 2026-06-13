//! The committed `.repoctx/hook.sh` template + its renderer.
//!
//! The script is a dumb pipe: it bootstraps + execs `repoctx hook claude`.
//! All rewrite/JSON/chain logic lives in the binary. The template is
//! embedded via `include_str!`; `render` substitutes the three `{{...}}`
//! variables. `init` writes the rendered script; `doctor` re-renders and
//! byte-compares to detect drift. See
//! `wiki/decisions/2026-06-13-repoctx-init.md`.

#![allow(dead_code)] // wired up by init (4b2af2a) + doctor (2307c32)

/// Marker version stamped into the generated script (line 2:
/// `# repoctx-hook-version: <N>`). Bump when the template shape changes
/// in a way `doctor` must treat as a regeneration.
pub const MARKER_VERSION: u32 = 1;

const TEMPLATE: &str = include_str!("../assets/hook.sh.tmpl");

/// Render the hook script.
///
/// - `rtk_chain` — bake `RTK_CHAIN=0|1` (chain rtk on passthrough).
/// - `min_version` — the binary version generating the script.
/// - `repoctx_bin` — `"repoctx"` (PATH lookup) or an absolute path.
pub fn render(rtk_chain: bool, min_version: &str, repoctx_bin: &str) -> String {
    TEMPLATE
        .replace("{{RTK_CHAIN}}", if rtk_chain { "1" } else { "0" })
        .replace("{{MIN_VERSION}}", min_version)
        .replace("{{REPOCTX_BIN}}", repoctx_bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_substitutions() {
        let s = render(true, "0.6.0", "repoctx");
        assert!(s.contains("RTK_CHAIN=1"));
        assert!(s.contains("MIN_VERSION=\"0.6.0\""));
        assert!(s.contains("REPOCTX=\"repoctx\""));
        assert!(!s.contains("{{"), "all template vars substituted");
    }

    #[test]
    fn rtk_chain_off_renders_zero() {
        let s = render(false, "0.6.0", "/usr/bin/repoctx");
        assert!(s.contains("RTK_CHAIN=0"));
        assert!(s.contains("REPOCTX=\"/usr/bin/repoctx\""));
    }

    #[test]
    fn carries_version_marker_on_line_two() {
        let s = render(false, "0.6.0", "repoctx");
        let line2 = s.lines().nth(1).unwrap();
        assert_eq!(line2, format!("# repoctx-hook-version: {MARKER_VERSION}"));
    }

    #[test]
    fn execs_hook_claude_with_flag() {
        let s = render(true, "0.6.0", "repoctx");
        assert!(s.contains(r#"exec "$REPOCTX" hook claude --rtk-chain="$RTK_CHAIN""#));
    }
}
