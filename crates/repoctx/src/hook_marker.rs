//! Fingerprint-marker reader for hook scripts.
//!
//! repoctx and rtk both stamp their generated `PreToolUse` hook scripts
//! with a second-line marker:
//!
//! ```text
//! #!/usr/bin/env bash
//! # repoctx-hook-version: 1
//! ```
//!
//! Reading it lets us classify a `command` that points at a script file
//! (ours vs rtk vs foreign) without executing it, and lets `doctor`
//! report the installed-tool version per scope. See the design doc
//! `wiki/decisions/2026-06-13-repoctx-init.md` and issues `b2ad123`
//! (foreign-hook detection) and `2307c32` (doctor).
#![allow(dead_code)] // consumers land in b2ad123 (foreign-hook detection) + 2307c32 (doctor)

use std::path::Path;

/// A parsed hook-script fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookMarker {
    /// Tool that generated the script, e.g. `repoctx` or `rtk`.
    pub tool: String,
    /// Marker version integer.
    pub version: u32,
}

/// Parse a marker from script text. Scans up to the first 8 lines,
/// skipping a leading shebang and blank lines; returns the first
/// `# <tool>-hook-version: <n>` comment found. Stops at the first line
/// that is neither a shebang, blank, nor a `#` comment (i.e. real code),
/// so we never read deep into an arbitrary script.
pub fn parse(text: &str) -> Option<HookMarker> {
    for line in text.lines().take(8) {
        let t = line.trim();
        if t.is_empty() || t.starts_with("#!") {
            continue; // shebang / blank — keep scanning the header
        }
        if !t.starts_with('#') {
            return None; // first real line — markers live in the header
        }
        if let Some(m) = parse_marker_line(t) {
            return Some(m);
        }
        // a non-marker comment — keep scanning the comment header
    }
    None
}

/// Read a marker from a file. Returns `None` if the file is unreadable
/// or not valid UTF-8 (e.g. a binary).
pub fn read(path: &Path) -> Option<HookMarker> {
    let text = std::fs::read_to_string(path).ok()?;
    parse(&text)
}

/// Match exactly `# <tool>-hook-version: <n>` on one already-trimmed
/// comment line. `<tool>` is `[a-z]+`, `<n>` is a `u32`. Hand-parsed to
/// avoid a regex dependency.
fn parse_marker_line(line: &str) -> Option<HookMarker> {
    const SUFFIX: &str = "-hook-version:";

    // Strip the leading '#' and the required whitespace after it.
    let rest = line.strip_prefix('#')?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();

    // tool = leading run of ascii-lowercase, then the literal suffix.
    let tool_len = rest.chars().take_while(|c| c.is_ascii_lowercase()).count();
    if tool_len == 0 {
        return None;
    }
    let (tool, after_tool) = rest.split_at(tool_len);
    let after_suffix = after_tool.strip_prefix(SUFFIX)?;

    // whitespace, then the version digits, then only trailing whitespace.
    let after_ws = after_suffix.trim_start();
    if after_ws.len() == after_suffix.len() {
        return None; // suffix must be followed by whitespace
    }
    let digits: String = after_ws
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    if !after_ws[digits.len()..].trim().is_empty() {
        return None; // trailing junk after the number
    }
    let version = digits.parse().ok()?;
    Some(HookMarker {
        tool: tool.to_string(),
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repoctx_marker_after_shebang() {
        let s = "#!/usr/bin/env bash\n# repoctx-hook-version: 1\nexec repoctx hook claude\n";
        assert_eq!(
            parse(s),
            Some(HookMarker {
                tool: "repoctx".into(),
                version: 1
            })
        );
    }

    #[test]
    fn parses_rtk_marker() {
        let s = "#!/usr/bin/env bash\n# rtk-hook-version: 3\n# more comments\n";
        assert_eq!(
            parse(s),
            Some(HookMarker {
                tool: "rtk".into(),
                version: 3
            })
        );
    }

    #[test]
    fn skips_extra_comment_lines_before_marker() {
        let s = "#!/bin/sh\n# generated file\n# do not edit\n# repoctx-hook-version: 2\n";
        assert_eq!(parse(s).unwrap().version, 2);
    }

    #[test]
    fn none_without_marker() {
        assert_eq!(parse("#!/usr/bin/env bash\necho hi\n"), None);
        assert_eq!(parse("# just a comment\nexec foo\n"), None);
    }

    #[test]
    fn none_when_marker_is_past_the_header() {
        // Real code appears before the marker → stop scanning.
        let s = "echo start\n# repoctx-hook-version: 1\n";
        assert_eq!(parse(s), None);
    }

    #[test]
    fn none_beyond_eight_lines() {
        let mut s = String::from("#!/bin/sh\n");
        for _ in 0..8 {
            s.push_str("# filler\n");
        }
        s.push_str("# repoctx-hook-version: 1\n");
        assert_eq!(parse(&s), None);
    }

    #[test]
    fn rejects_malformed_markers() {
        assert_eq!(parse("# repoctx-hook-version:\n"), None); // no number
        assert_eq!(parse("# repoctx-hook-version: x\n"), None); // non-digit
        assert_eq!(parse("# Repoctx-hook-version: 1\n"), None); // uppercase tool
        assert_eq!(parse("#repoctx-hook-version: 1\n"), None); // no space after #
        assert_eq!(parse("# repoctx-hook-version: 1 extra\n"), None); // trailing junk
    }

    #[test]
    fn read_missing_file_is_none() {
        assert_eq!(read(Path::new("/no/such/hook.sh")), None);
    }

    #[test]
    fn read_binary_file_is_none() {
        // Non-UTF-8 bytes → read_to_string fails → None.
        let dir = std::env::temp_dir();
        let path = dir.join("repoctx-marker-bintest.bin");
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        assert_eq!(read(&path), None);
        let _ = std::fs::remove_file(&path);
    }
}
