//! End-to-end coverage for `repoctx hook install`.
//!
//! Strategy: redirect the binary's XDG cache via `XDG_CACHE_HOME` to a
//! tempdir, pre-seed the cache with a tiny in-tree agent, then drive
//! the CLI. The Fetcher serves from cache on every call, so these
//! tests never touch the network.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const REF: &str = "test-ref";

fn seed_cache(cache_root: &Path, agent: &str, files: &[(&str, &str)]) {
    let dir = cache_root
        .join("repoctx/integrations")
        .join(REF)
        .join(agent);
    fs::create_dir_all(&dir).unwrap();
    for (name, body) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }
}

fn seed_shared(cache_root: &Path, file: &str, body: &str) {
    let dir = cache_root
        .join("repoctx/integrations")
        .join(REF)
        .join("shared");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(file), body).unwrap();
}

fn run(cache_root: &Path, target: &Path, extra_args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("XDG_CACHE_HOME", cache_root)
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .args(["--json", "hook"])
        .args(extra_args)
        .arg("--dir")
        .arg(target)
        .arg("--ref")
        .arg(REF);
    cmd.assert()
}

const WRITE_MANIFEST: &str = r#"
name = "claude"
description = "test"
[[file]]
src = "SKILL.md"
dest = ".claude/skills/repoctx/SKILL.md"
mode = "write"
"#;

const SKILL: &str = "# repoctx skill\n{REPOCTX_BIN}\n";

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

fn fixture_write() -> (TempDir, TempDir) {
    let cache = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    seed_cache(
        cache.path(),
        "claude",
        &[("manifest.toml", WRITE_MANIFEST), ("SKILL.md", SKILL)],
    );
    (cache, target)
}

fn json(out: &[u8]) -> Value {
    serde_json::from_slice(out).unwrap()
}

#[test]
fn clean_install_writes_files() {
    let (cache, target) = fixture_write();
    let assert = run(cache.path(), target.path(), &["install", "claude"]).success();
    let v: Value = json(&assert.get_output().stdout);
    assert_eq!(v["agent"], "claude");
    assert_eq!(v["dry_run"], false);
    let written = v["written"].as_array().unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["action"], "created");
    let skill = target.path().join(".claude/skills/repoctx/SKILL.md");
    assert!(skill.exists());
    let body = fs::read_to_string(&skill).unwrap();
    // {REPOCTX_BIN} substituted to something path-like.
    assert!(body.contains("repoctx"));
    assert!(!body.contains("{REPOCTX_BIN}"));
}

#[test]
fn dry_run_writes_nothing() {
    let (cache, target) = fixture_write();
    let assert = run(
        cache.path(),
        target.path(),
        &["install", "claude", "--dry-run"],
    )
    .success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "dry_run");
    assert!(!target
        .path()
        .join(".claude/skills/repoctx/SKILL.md")
        .exists());
}

#[test]
fn idempotent_reinstall_reports_skipped_identical() {
    let (cache, target) = fixture_write();
    run(cache.path(), target.path(), &["install", "claude"]).success();
    let assert = run(cache.path(), target.path(), &["install", "claude"]).success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "skipped_identical");
}

#[test]
fn force_required_to_overwrite_local_edit() {
    let (cache, target) = fixture_write();
    run(cache.path(), target.path(), &["install", "claude"]).success();
    let skill = target.path().join(".claude/skills/repoctx/SKILL.md");
    fs::write(&skill, "edited locally\n").unwrap();
    // Without --force: error.
    run(cache.path(), target.path(), &["install", "claude"]).failure();
    // With --force: Updated.
    let assert = run(
        cache.path(),
        target.path(),
        &["install", "claude", "--force"],
    )
    .success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "updated");
}

#[test]
fn ref_override_picks_a_different_cache_subdir() {
    // Two refs in cache; --ref selects which to read.
    let cache = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let a_dir = cache.path().join("repoctx/integrations/aaa/claude");
    let b_dir = cache.path().join("repoctx/integrations/bbb/claude");
    fs::create_dir_all(&a_dir).unwrap();
    fs::create_dir_all(&b_dir).unwrap();
    fs::write(a_dir.join("manifest.toml"), WRITE_MANIFEST).unwrap();
    fs::write(a_dir.join("SKILL.md"), "FROM_AAA").unwrap();
    fs::write(b_dir.join("manifest.toml"), WRITE_MANIFEST).unwrap();
    fs::write(b_dir.join("SKILL.md"), "FROM_BBB").unwrap();

    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("XDG_CACHE_HOME", cache.path())
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .args(["--json", "hook", "install", "claude"])
        .arg("--dir")
        .arg(target.path())
        .args(["--ref", "bbb"]);
    cmd.assert().success();
    let skill = fs::read_to_string(target.path().join(".claude/skills/repoctx/SKILL.md")).unwrap();
    assert_eq!(skill, "FROM_BBB");
}

#[test]
fn merge_section_install_then_idempotent() {
    let cache = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    seed_cache(cache.path(), "codex", &[("manifest.toml", MERGE_MANIFEST)]);
    seed_shared(cache.path(), "AGENTS.md.fragment", "shared body\n");

    let assert = run(cache.path(), target.path(), &["install", "codex"]).success();
    let v = json(&assert.get_output().stdout);
    assert_eq!(v["written"][0]["action"], "created");
    let agents = fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("<!-- repoctx:start -->\nshared body"));

    let assert2 = run(cache.path(), target.path(), &["install", "codex"]).success();
    let v2 = json(&assert2.get_output().stdout);
    assert_eq!(v2["written"][0]["action"], "skipped_identical");
}

#[test]
fn unknown_agent_errors_with_known_list() {
    let cache = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("XDG_CACHE_HOME", cache.path())
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .args(["hook", "install", "aider"])
        .arg("--dir")
        .arg(target.path());
    let output = cmd.assert().failure().get_output().stderr.clone();
    let s = String::from_utf8_lossy(&output);
    assert!(s.contains("unknown agent: aider"));
    assert!(s.contains("claude"));
    assert!(s.contains("codex"));
    assert!(s.contains("opencode"));
}

#[test]
fn hook_list_returns_three_agents() {
    let cache = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("repoctx").unwrap();
    cmd.env("XDG_CACHE_HOME", cache.path())
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .args(["--json", "hook", "list", "--ref", REF, "--no-cache"]);
    // No fixtures and --no-cache → manifest fetches will 404; descriptions
    // are best-effort and the command still succeeds.
    let output = cmd.assert().success().get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(v["count"], 3);
    let names: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["claude", "codex", "opencode"]);
}
