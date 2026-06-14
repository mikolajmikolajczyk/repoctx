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

pub struct Installer {
    dir: PathBuf,
    force: bool,
    dry_run: bool,
    global: bool,
    vars: BTreeMap<String, String>,
}

impl Installer {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            force: false,
            dry_run: false,
            global: false,
            vars: BTreeMap::new(),
        }
    }

    pub fn force(mut self, on: bool) -> Self {
        self.force = on;
        self
    }

    /// User-global scope: install only files whose dest lives under
    /// `.claude/` (the skill), skipping repo-root guidance like `AGENTS.md`
    /// that only makes sense inside a project. `dir` should be the home
    /// directory so `.claude/skills/…` resolves to `~/.claude/skills/…`.
    pub fn global(mut self, on: bool) -> Self {
        self.global = on;
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
        let manifest = crate::content::manifest(agent)?;
        let files: Vec<File> = manifest
            .files
            .into_iter()
            .filter(|f| !self.global || is_global_scoped(&f.dest))
            .collect();
        let mut written = Vec::with_capacity(files.len());
        for file in &files {
            let bytes = crate::content::file(agent, &file.src)?;
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
        let removal = removal_recipe(agent, &files);
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

/// A manifest dest is valid at user-global scope when it lives under a
/// skills directory the agent reads from a home dir (`~/.claude/skills/`,
/// `~/.agents/skills/`). Repo-root files like `AGENTS.md` / `CLAUDE.md` are
/// project-only and skipped for a global install.
fn is_global_scoped(dest: &str) -> bool {
    dest.starts_with(".claude/skills/") || dest.starts_with(".agents/skills/")
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

    fn write_file(dest: &str) -> File {
        File {
            src: "x".into(),
            dest: dest.into(),
            mode: Mode::Write,
            start_marker: None,
            end_marker: None,
        }
    }

    fn merge_file(dest: &str) -> File {
        File {
            src: "x".into(),
            dest: dest.into(),
            mode: Mode::MergeSection,
            start_marker: Some("<!-- repoctx:start -->".into()),
            end_marker: Some("<!-- repoctx:end -->".into()),
        }
    }

    fn append_file(dest: &str, marker: &str) -> File {
        File {
            src: "x".into(),
            dest: dest.into(),
            mode: Mode::Append,
            start_marker: Some(marker.into()),
            end_marker: None,
        }
    }

    /// Run one file through dispatch (the write/merge/append core), with
    /// template vars applied like the real install path.
    fn run(inst: &Installer, file: &File, content: &str) -> Result<WriteAction> {
        let text = inst.apply_vars(content);
        inst.dispatch(file, text.as_bytes())
    }

    #[test]
    fn embedded_install_claude_writes_skill_and_merges_claude_md() {
        // Full end-to-end over the *real* embedded content (no stubs).
        let target = tempfile::tempdir().unwrap();
        let r = Installer::new(target.path().to_path_buf())
            .var("REPOCTX_BIN", "/usr/bin/repoctx")
            .install("claude")
            .unwrap();
        assert_eq!(r.written.len(), 2);
        assert!(target
            .path()
            .join(".claude/skills/repoctx/SKILL.md")
            .exists());
        let claude_md = fs::read_to_string(target.path().join("CLAUDE.md")).unwrap();
        assert!(claude_md.contains("<!-- repoctx:start -->"));
        assert!(claude_md.contains("<!-- repoctx:end -->"));
    }

    #[test]
    fn embedded_install_is_idempotent() {
        let target = tempfile::tempdir().unwrap();
        Installer::new(target.path().to_path_buf())
            .install("codex")
            .unwrap();
        let r2 = Installer::new(target.path().to_path_buf())
            .install("codex")
            .unwrap();
        assert!(r2
            .written
            .iter()
            .all(|w| matches!(w.action, Action::SkippedIdentical)));
    }

    #[test]
    fn write_mode_creates() {
        let target = tempfile::tempdir().unwrap();
        let inst =
            Installer::new(target.path().to_path_buf()).var("REPOCTX_BIN", "/usr/bin/repoctx");
        let f = write_file(".claude/skills/repoctx/SKILL.md");
        let a = run(&inst, &f, "hello {REPOCTX_BIN}").unwrap();
        assert_eq!(a.action, Action::Created);
        let written =
            fs::read_to_string(target.path().join(".claude/skills/repoctx/SKILL.md")).unwrap();
        assert_eq!(written, "hello /usr/bin/repoctx");
    }

    #[test]
    fn write_mode_idempotent_on_second_install() {
        let target = tempfile::tempdir().unwrap();
        let inst = Installer::new(target.path().to_path_buf());
        let f = write_file("a/SKILL.md");
        run(&inst, &f, "stable").unwrap();
        let a2 = run(&inst, &f, "stable").unwrap();
        assert_eq!(a2.action, Action::SkippedIdentical);
    }

    #[test]
    fn write_mode_refuses_diff_without_force() {
        let target = tempfile::tempdir().unwrap();
        let dest = target.path().join("a/SKILL.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "edited locally").unwrap();
        let inst = Installer::new(target.path().to_path_buf());
        let err = run(&inst, &write_file("a/SKILL.md"), "upstream").unwrap_err();
        assert!(matches!(err, IntegrationsError::WriteRefused { .. }));
    }

    #[test]
    fn write_mode_force_updates() {
        let target = tempfile::tempdir().unwrap();
        let dest = target.path().join("a/SKILL.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "edited locally").unwrap();
        let inst = Installer::new(target.path().to_path_buf()).force(true);
        let a = run(&inst, &write_file("a/SKILL.md"), "upstream").unwrap();
        assert_eq!(a.action, Action::Updated);
        assert_eq!(fs::read_to_string(&dest).unwrap(), "upstream");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let target = tempfile::tempdir().unwrap();
        let inst = Installer::new(target.path().to_path_buf()).dry_run(true);
        let a = run(&inst, &write_file("a/SKILL.md"), "hello").unwrap();
        assert_eq!(a.action, Action::DryRun);
        assert!(!target.path().join("a/SKILL.md").exists());
    }

    #[test]
    fn merge_section_creates_and_replaces() {
        let target = tempfile::tempdir().unwrap();
        let inst = Installer::new(target.path().to_path_buf());
        let f = merge_file("AGENTS.md");
        let a = run(&inst, &f, "v1").unwrap();
        assert_eq!(a.action, Action::Created);
        let content = fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
        assert!(content.contains("<!-- repoctx:start -->\nv1\n<!-- repoctx:end -->"));

        // re-install with new content → ReplacedSection, surrounding text preserved.
        fs::write(
            target.path().join("AGENTS.md"),
            "prefix\n<!-- repoctx:start -->\nOLD\n<!-- repoctx:end -->\nsuffix",
        )
        .unwrap();
        let a2 = run(&inst, &f, "v2").unwrap();
        assert_eq!(a2.action, Action::ReplacedSection);
        let after = fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
        assert!(after.starts_with("prefix\n"));
        assert!(after.ends_with("suffix"));
        assert!(after.contains("v2"));
        assert!(!after.contains("OLD"));
    }

    #[test]
    fn merge_section_idempotent() {
        let target = tempfile::tempdir().unwrap();
        let inst = Installer::new(target.path().to_path_buf());
        let f = merge_file("AGENTS.md");
        run(&inst, &f, "v1").unwrap();
        let a2 = run(&inst, &f, "v1").unwrap();
        assert_eq!(a2.action, Action::SkippedIdentical);
    }

    #[test]
    fn append_mode_idempotent_via_marker() {
        let target = tempfile::tempdir().unwrap();
        fs::write(
            target.path().join("NOTES.md"),
            "preexisting\n## repoctx\nbody\n",
        )
        .unwrap();
        let inst = Installer::new(target.path().to_path_buf());
        let a = run(
            &inst,
            &append_file("NOTES.md", "## repoctx"),
            "## repoctx\nbody\n",
        )
        .unwrap();
        assert_eq!(a.action, Action::SkippedMarkerPresent);
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
