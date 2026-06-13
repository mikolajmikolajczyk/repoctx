//! End-to-end: the rendered `.repoctx/hook.sh` actually executed by a
//! shell (0a338d7). Proves the committed dumb-pipe script behaves across
//! the RTK_CHAIN × repoctx-present × rtk-present matrix — the one thing
//! the Rust-level tests can't reach.
//!
//! Sibling suites cover the rest of the matrix:
//!   - rewrite decisions: rewrite_corpus.rs (via `hook claude`)
//!   - scope races / foreign refusal / migration: init_cmd.rs
//!   - drift + repair: doctor.rs ; uninstall: uninstall.rs
//!
//! Skips cleanly where `bash` is absent (e.g. a Windows runner without
//! Git Bash) — via a runtime probe, not a compile-time OS gate, to honor
//! the platform-agnostic policy.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;

fn bash() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("bash"))
        .find(|p| p.is_file())
}

fn repoctx_bin() -> PathBuf {
    Command::cargo_bin("repoctx").unwrap().get_program().into()
}

fn make_exec(path: &Path) {
    let _ = Command::new("chmod").arg("+x").arg(path).status();
}

/// Render `.repoctx/hook.sh` via `repoctx init` into a fresh repo.
fn render_script(repo: &Path, home: &Path, rtk_flag: &str) {
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .args(["init", "--yes", "--rtk", rtk_flag])
        .output()
        .unwrap();
}

/// A fake `rtk` on PATH that consumes stdin and emits a canned rewrite.
fn write_rtk_stub(dir: &Path) {
    let p = dir.join("rtk");
    std::fs::write(
        &p,
        "#!/usr/bin/env bash\ncat >/dev/null\necho '{\"rtk\":\"handled\"}'\nexit 0\n",
    )
    .unwrap();
    make_exec(&p);
}

/// Run the rendered script under `bash` with a controlled PATH + stdin.
fn run_script(repo: &Path, home: &Path, path_env: &str, stdin: &str) -> (i32, String, String) {
    let script = repo.join(".repoctx/hook.sh");
    let out = Command::new("bash")
        .arg(&script)
        .current_dir(repo)
        .env("PATH", path_env)
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(stdin.as_bytes())?;
            c.wait_with_output()
        })
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn sys_base() -> String {
    "/usr/bin:/bin".to_string()
}

#[test]
fn rendered_script_rewrites_rg() {
    if bash().is_none() {
        eprintln!("skip: bash not available");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "off");

    let target = repoctx_bin().parent().unwrap().to_path_buf();
    let path = format!("{}:{}", target.display(), sys_base());
    let (code, stdout, _) = run_script(
        repo.path(),
        home.path(),
        &path,
        r#"{"tool_input":{"command":"rg parseConfig"}}"#,
    );
    assert_eq!(code, 0, "rewrite exits 0");
    assert!(
        stdout.contains("repoctx symbols parseConfig --json"),
        "stdout={stdout}"
    );
}

#[test]
fn rendered_script_passthrough_no_chain() {
    if bash().is_none() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "off"); // RTK_CHAIN=0

    let target = repoctx_bin().parent().unwrap().to_path_buf();
    let path = format!("{}:{}", target.display(), sys_base());
    let (code, stdout, _) = run_script(
        repo.path(),
        home.path(),
        &path,
        r#"{"tool_input":{"command":"ls -la"}}"#,
    );
    assert_eq!(code, 1, "passthrough exits 1");
    assert!(stdout.trim().is_empty(), "no payload: {stdout}");
}

#[test]
fn rendered_script_chains_rtk_on_passthrough() {
    if bash().is_none() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "on"); // RTK_CHAIN=1

    let stubdir = tempfile::tempdir().unwrap();
    write_rtk_stub(stubdir.path());
    let target = repoctx_bin().parent().unwrap().to_path_buf();
    // repoctx present + rtk stub present.
    let path = format!(
        "{}:{}:{}",
        stubdir.path().display(),
        target.display(),
        sys_base()
    );
    let (code, stdout, _) = run_script(
        repo.path(),
        home.path(),
        &path,
        r#"{"tool_input":{"command":"ls -la"}}"#,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains(r#"{"rtk":"handled"}"#), "stdout={stdout}");
}

#[test]
fn rendered_script_repoctx_missing_execs_rtk() {
    if bash().is_none() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "on"); // RTK_CHAIN=1

    let stubdir = tempfile::tempdir().unwrap();
    write_rtk_stub(stubdir.path());
    // No repoctx on PATH (omit the target dir); rtk stub present.
    let path = format!("{}:{}", stubdir.path().display(), sys_base());
    let (code, stdout, stderr) = run_script(
        repo.path(),
        home.path(),
        &path,
        r#"{"tool_input":{"command":"ls -la"}}"#,
    );
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains(r#"{"rtk":"handled"}"#), "stdout={stdout}");
    assert!(stderr.contains("not installed"), "install hint: {stderr}");
}

#[test]
fn rendered_script_repoctx_missing_passthrough() {
    if bash().is_none() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "off"); // RTK_CHAIN=0

    // No repoctx, no rtk.
    let path = sys_base();
    let (code, stdout, stderr) = run_script(
        repo.path(),
        home.path(),
        &path,
        r#"{"tool_input":{"command":"ls -la"}}"#,
    );
    assert_eq!(
        code, 0,
        "missing binary must passthrough (never block Bash)"
    );
    assert!(stdout.trim().is_empty());
    assert!(stderr.contains("not installed"), "install hint: {stderr}");
}

#[test]
fn rendered_script_is_executable() {
    if bash().is_none() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    render_script(repo.path(), home.path(), "off");

    // Run it directly (not via `bash <script>`) — proves the +x bit.
    let target = repoctx_bin().parent().unwrap().to_path_buf();
    let path = format!("{}:{}", target.display(), sys_base());
    let out = Command::new(repo.path().join(".repoctx/hook.sh"))
        .current_dir(repo.path())
        .env("PATH", path)
        .env("HOME", home.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .take()
                .unwrap()
                .write_all(br#"{"tool_input":{"command":"rg parseConfig"}}"#)?;
            c.wait_with_output()
        })
        .unwrap();
    assert_eq!(out.status.code().unwrap_or(-1), 0);
    assert!(String::from_utf8_lossy(&out.stdout).contains("repoctx symbols parseConfig"));
}
