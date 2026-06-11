//! Install per-agent files into a target directory.
//!
//! Three write modes (matches `manifest::Mode`):
//!
//! - `Write` — create the dest, or overwrite when caller passes `force=true`.
//!   Identical existing content is a no-op (`SkippedIdentical`). Differing
//!   content without `force` is an error.
//! - `Append` — add the source bytes once. Idempotency probe: dest already
//!   contains `start_marker`.
//! - `MergeSection` — replace the block between `start_marker` and the next
//!   `end_marker`, or append a fresh wrapped block when markers are absent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{IntegrationsError, Result};
use crate::fetcher::Fetcher;
use crate::manifest::{File, Mode};

/// One file's outcome.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WriteAction {
    pub path: PathBuf,
    pub mode: String,
    pub bytes: usize,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Created,
    Updated,
    ReplacedSection,
    Appended,
    SkippedIdentical,
    SkippedMarkerPresent,
    DryRun,
}

/// Whole-install summary.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub agent: String,
    pub dir: PathBuf,
    pub dry_run: bool,
    pub force: bool,
    pub written: Vec<WriteAction>,
    pub removal: String,
}

pub struct Installer<'a> {
    fetcher: &'a Fetcher,
    dir: PathBuf,
    force: bool,
    dry_run: bool,
    vars: BTreeMap<String, String>,
}

impl<'a> Installer<'a> {
    pub fn new(fetcher: &'a Fetcher, dir: PathBuf) -> Self {
        Self {
            fetcher,
            dir,
            force: false,
            dry_run: false,
            vars: BTreeMap::new(),
        }
    }

    pub fn force(mut self, on: bool) -> Self {
        self.force = on;
        self
    }

    pub fn dry_run(mut self, on: bool) -> Self {
        self.dry_run = on;
        self
    }

    /// Provide a template variable like `("REPOCTX_BIN", "/usr/bin/repoctx")`.
    /// Resolved as `{KEY}` literal substring in source content.
    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn install(self, agent: &str) -> Result<InstallResult> {
        let manifest = self.fetcher.fetch_manifest(agent)?;
        let mut written = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            let bytes = self.fetcher.fetch_file(agent, &file.src)?;
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => self.apply_vars(s),
                Err(_) => {
                    // Binary fragment — write verbatim, no template subst.
                    return self.write_binary(agent, file, &bytes, written);
                }
            };
            let action = self.dispatch(file, text.as_bytes())?;
            written.push(action);
        }
        let removal = removal_recipe(agent, &manifest.files);
        Ok(InstallResult {
            agent: agent.to_string(),
            dir: self.dir.clone(),
            dry_run: self.dry_run,
            force: self.force,
            written,
            removal,
        })
    }

    fn write_binary(
        &self,
        agent: &str,
        file: &File,
        bytes: &[u8],
        mut written: Vec<WriteAction>,
    ) -> Result<InstallResult> {
        if file.mode != Mode::Write {
            return Err(IntegrationsError::ManifestInvalid {
                path: PathBuf::from(&file.dest),
                reason: format!(
                    "non-UTF-8 source `{}` requires mode=write (got {:?})",
                    file.src, file.mode
                ),
            });
        }
        let action = self.dispatch(file, bytes)?;
        written.push(action);
        Ok(InstallResult {
            agent: agent.to_string(),
            dir: self.dir.clone(),
            dry_run: self.dry_run,
            force: self.force,
            removal: String::new(),
            written,
        })
    }

    fn apply_vars(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (k, v) in &self.vars {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }

    fn dispatch(&self, file: &File, bytes: &[u8]) -> Result<WriteAction> {
        let dest = self.dir.join(&file.dest);
        match file.mode {
            Mode::Write => self.do_write(&dest, file, bytes),
            Mode::Append => self.do_append(&dest, file, bytes),
            Mode::MergeSection => self.do_merge_section(&dest, file, bytes),
        }
    }

    fn do_write(&self, dest: &Path, file: &File, bytes: &[u8]) -> Result<WriteAction> {
        let existing = fs::read(dest).ok();
        let action = match &existing {
            Some(cur) if cur.as_slice() == bytes => Action::SkippedIdentical,
            Some(_) if !self.force => {
                return Err(IntegrationsError::WriteRefused {
                    path: dest.to_path_buf(),
                    reason: "destination exists with different content; pass --force to overwrite"
                        .into(),
                });
            }
            Some(_) => Action::Updated,
            None => Action::Created,
        };
        self.write_bytes(dest, bytes, action, file)
    }

    fn do_append(&self, dest: &Path, file: &File, bytes: &[u8]) -> Result<WriteAction> {
        let marker = file.start_marker.as_deref().expect("validated");
        let existing = fs::read_to_string(dest).ok();
        if let Some(cur) = &existing {
            if cur.contains(marker) {
                return self.write_bytes(dest, bytes, Action::SkippedMarkerPresent, file);
            }
        }
        let mut new_content = existing.unwrap_or_default();
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(std::str::from_utf8(bytes).map_err(|e| {
            IntegrationsError::ManifestInvalid {
                path: dest.to_path_buf(),
                reason: format!("append source is not UTF-8: {e}"),
            }
        })?);
        self.write_bytes(dest, new_content.as_bytes(), Action::Appended, file)
    }

    fn do_merge_section(&self, dest: &Path, file: &File, bytes: &[u8]) -> Result<WriteAction> {
        let start = file.start_marker.as_deref().expect("validated");
        let end = file.end_marker.as_deref().expect("validated");
        let block_body =
            std::str::from_utf8(bytes).map_err(|e| IntegrationsError::ManifestInvalid {
                path: dest.to_path_buf(),
                reason: format!("merge-section source is not UTF-8: {e}"),
            })?;
        let wrapped = format!("{start}\n{block_body}\n{end}");
        let existing = fs::read_to_string(dest).ok();
        match existing {
            None => self.write_bytes(dest, wrapped.as_bytes(), Action::Created, file),
            Some(cur) => match find_section(&cur, start, end) {
                Some((from, to)) => {
                    let mut new_content = String::with_capacity(cur.len() + wrapped.len());
                    new_content.push_str(&cur[..from]);
                    new_content.push_str(&wrapped);
                    new_content.push_str(&cur[to..]);
                    if new_content == cur {
                        return self.write_bytes(
                            dest,
                            new_content.as_bytes(),
                            Action::SkippedIdentical,
                            file,
                        );
                    }
                    self.write_bytes(dest, new_content.as_bytes(), Action::ReplacedSection, file)
                }
                None => {
                    let mut new_content = cur.clone();
                    if !new_content.is_empty() && !new_content.ends_with('\n') {
                        new_content.push('\n');
                    }
                    new_content.push_str(&wrapped);
                    self.write_bytes(dest, new_content.as_bytes(), Action::Appended, file)
                }
            },
        }
    }

    fn write_bytes(
        &self,
        dest: &Path,
        bytes: &[u8],
        action: Action,
        file: &File,
    ) -> Result<WriteAction> {
        let real_action = if self.dry_run { Action::DryRun } else { action };
        if !self.dry_run
            && !matches!(
                action,
                Action::SkippedIdentical | Action::SkippedMarkerPresent
            )
        {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| IntegrationsError::Io {
                    path: parent.to_path_buf(),
                    reason: e.to_string(),
                })?;
            }
            fs::write(dest, bytes).map_err(|e| IntegrationsError::Io {
                path: dest.to_path_buf(),
                reason: e.to_string(),
            })?;
        }
        Ok(WriteAction {
            path: dest.to_path_buf(),
            mode: mode_str(file.mode).into(),
            bytes: bytes.len(),
            action: real_action,
        })
    }
}

fn find_section(haystack: &str, start: &str, end: &str) -> Option<(usize, usize)> {
    let s = haystack.find(start)?;
    let after_start = s + start.len();
    let e_rel = haystack[after_start..].find(end)?;
    Some((s, after_start + e_rel + end.len()))
}

fn removal_recipe(agent: &str, files: &[File]) -> String {
    let mut out = format!("Installed {agent}. To remove:\n");
    for f in files {
        match f.mode {
            Mode::Write => {
                out.push_str(&format!("  - rm {}\n", f.dest));
            }
            Mode::Append => {
                let m = f.start_marker.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "  - in {}, delete the appended block (starts at `{}`)\n",
                    f.dest, m
                ));
            }
            Mode::MergeSection => {
                let s = f.start_marker.as_deref().unwrap_or("");
                let e = f.end_marker.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "  - in {}, delete the block between `{}` and `{}` (inclusive)\n",
                    f.dest, s, e
                ));
            }
        }
    }
    out.trim_end().to_string()
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Write => "write",
        Mode::Append => "append",
        Mode::MergeSection => "merge-section",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetcher::HttpFetch;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Stub {
        files: HashMap<String, Vec<u8>>,
        _calls: Mutex<Vec<String>>,
    }

    impl Stub {
        fn with(files: &[(&str, &str)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                    .collect(),
                _calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl HttpFetch for Stub {
        fn get(&self, url: &str) -> std::result::Result<Vec<u8>, String> {
            self.files
                .get(url)
                .cloned()
                .ok_or_else(|| format!("404 (test): {url}"))
        }
    }

    fn fetcher(stub: Stub, cache: &Path) -> Fetcher {
        // Tests use no_cache=true to keep each call deterministic;
        // cache-hit behavior is covered separately in fetcher tests.
        Fetcher::with_parts("https://x", cache, "main", true, Box::new(stub))
    }

    const WRITE_MANIFEST: &str = r#"
name = "claude"
description = "test"
[[file]]
src = "SKILL.md"
dest = ".claude/skills/repoctx/SKILL.md"
mode = "write"
"#;

    const MERGE_MANIFEST: &str = r#"
name = "codex"
description = "test"
[[file]]
src = "../shared/AGENTS.md.fragment"
dest = "AGENTS.md"
mode = "merge-section"
start_marker = "<!-- repoctx:start -->"
end_marker = "<!-- repoctx:end -->"
"#;

    const APPEND_MANIFEST: &str = r###"
name = "claude"
description = "test"
[[file]]
src = "frag.md"
dest = "NOTES.md"
mode = "append"
start_marker = "## repoctx"
"###;

    #[test]
    fn write_mode_creates() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            (
                "https://x/main/integrations/claude/SKILL.md",
                "hello {REPOCTX_BIN}",
            ),
        ]);
        let f = fetcher(stub, cache.path());
        let result = Installer::new(&f, target.path().to_path_buf())
            .var("REPOCTX_BIN", "/usr/bin/repoctx")
            .install("claude")
            .unwrap();
        assert_eq!(result.written.len(), 1);
        assert_eq!(result.written[0].action, Action::Created);
        let written =
            fs::read_to_string(target.path().join(".claude/skills/repoctx/SKILL.md")).unwrap();
        assert_eq!(written, "hello /usr/bin/repoctx");
    }

    #[test]
    fn write_mode_idempotent_on_second_install() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            ("https://x/main/integrations/claude/SKILL.md", "stable"),
        ]);
        let f = fetcher(stub, cache.path());
        Installer::new(&f, target.path().to_path_buf())
            .install("claude")
            .unwrap();
        let stub2 = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            ("https://x/main/integrations/claude/SKILL.md", "stable"),
        ]);
        let f2 = fetcher(stub2, cache.path());
        let r2 = Installer::new(&f2, target.path().to_path_buf())
            .install("claude")
            .unwrap();
        assert_eq!(r2.written[0].action, Action::SkippedIdentical);
    }

    #[test]
    fn write_mode_refuses_diff_without_force() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let dest = target.path().join(".claude/skills/repoctx/SKILL.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "edited locally").unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            ("https://x/main/integrations/claude/SKILL.md", "upstream"),
        ]);
        let f = fetcher(stub, cache.path());
        let err = Installer::new(&f, target.path().to_path_buf())
            .install("claude")
            .unwrap_err();
        assert!(matches!(err, IntegrationsError::WriteRefused { .. }));
    }

    #[test]
    fn write_mode_force_updates() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let dest = target.path().join(".claude/skills/repoctx/SKILL.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "edited locally").unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            ("https://x/main/integrations/claude/SKILL.md", "upstream"),
        ]);
        let f = fetcher(stub, cache.path());
        let r = Installer::new(&f, target.path().to_path_buf())
            .force(true)
            .install("claude")
            .unwrap();
        assert_eq!(r.written[0].action, Action::Updated);
        assert_eq!(fs::read_to_string(&dest).unwrap(), "upstream");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                WRITE_MANIFEST,
            ),
            ("https://x/main/integrations/claude/SKILL.md", "hello"),
        ]);
        let f = fetcher(stub, cache.path());
        let r = Installer::new(&f, target.path().to_path_buf())
            .dry_run(true)
            .install("claude")
            .unwrap();
        assert_eq!(r.written[0].action, Action::DryRun);
        assert!(!target
            .path()
            .join(".claude/skills/repoctx/SKILL.md")
            .exists());
    }

    #[test]
    fn merge_section_creates_and_replaces() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/codex/manifest.toml",
                MERGE_MANIFEST,
            ),
            (
                "https://x/main/integrations/shared/AGENTS.md.fragment",
                "v1",
            ),
        ]);
        let f = fetcher(stub, cache.path());
        let r = Installer::new(&f, target.path().to_path_buf())
            .install("codex")
            .unwrap();
        assert_eq!(r.written[0].action, Action::Created);
        let content = fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
        assert!(content.contains("<!-- repoctx:start -->\nv1\n<!-- repoctx:end -->"));

        // re-install with new content → ReplacedSection, surrounding text preserved.
        fs::write(
            target.path().join("AGENTS.md"),
            "prefix\n<!-- repoctx:start -->\nOLD\n<!-- repoctx:end -->\nsuffix",
        )
        .unwrap();
        let stub2 = Stub::with(&[
            (
                "https://x/main/integrations/codex/manifest.toml",
                MERGE_MANIFEST,
            ),
            (
                "https://x/main/integrations/shared/AGENTS.md.fragment",
                "v2",
            ),
        ]);
        let f2 = fetcher(stub2, cache.path());
        let r2 = Installer::new(&f2, target.path().to_path_buf())
            .install("codex")
            .unwrap();
        assert_eq!(r2.written[0].action, Action::ReplacedSection);
        let after = fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
        assert!(after.starts_with("prefix\n"));
        assert!(after.ends_with("suffix"));
        assert!(after.contains("v2"));
        assert!(!after.contains("OLD"));
    }

    #[test]
    fn merge_section_idempotent() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/codex/manifest.toml",
                MERGE_MANIFEST,
            ),
            (
                "https://x/main/integrations/shared/AGENTS.md.fragment",
                "v1",
            ),
        ]);
        let f = fetcher(stub, cache.path());
        Installer::new(&f, target.path().to_path_buf())
            .install("codex")
            .unwrap();
        let stub2 = Stub::with(&[
            (
                "https://x/main/integrations/codex/manifest.toml",
                MERGE_MANIFEST,
            ),
            (
                "https://x/main/integrations/shared/AGENTS.md.fragment",
                "v1",
            ),
        ]);
        let f2 = fetcher(stub2, cache.path());
        let r2 = Installer::new(&f2, target.path().to_path_buf())
            .install("codex")
            .unwrap();
        assert_eq!(r2.written[0].action, Action::SkippedIdentical);
    }

    #[test]
    fn append_mode_idempotent_via_marker() {
        let cache = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(
            target.path().join("NOTES.md"),
            "preexisting\n## repoctx\nbody\n",
        )
        .unwrap();
        let stub = Stub::with(&[
            (
                "https://x/main/integrations/claude/manifest.toml",
                APPEND_MANIFEST,
            ),
            (
                "https://x/main/integrations/claude/frag.md",
                "## repoctx\nbody\n",
            ),
        ]);
        let f = fetcher(stub, cache.path());
        let r = Installer::new(&f, target.path().to_path_buf())
            .install("claude")
            .unwrap();
        assert_eq!(r.written[0].action, Action::SkippedMarkerPresent);
    }

    #[test]
    fn removal_recipe_covers_every_mode() {
        let files = vec![
            File {
                src: "a".into(),
                dest: "A.md".into(),
                mode: Mode::Write,
                start_marker: None,
                end_marker: None,
            },
            File {
                src: "b".into(),
                dest: "B.md".into(),
                mode: Mode::MergeSection,
                start_marker: Some("S".into()),
                end_marker: Some("E".into()),
            },
            File {
                src: "c".into(),
                dest: "C.md".into(),
                mode: Mode::Append,
                start_marker: Some("M".into()),
                end_marker: None,
            },
        ];
        let r = removal_recipe("claude", &files);
        assert!(r.contains("rm A.md"));
        assert!(r.contains("between `S` and `E`"));
        assert!(r.contains("starts at `M`"));
    }
}
