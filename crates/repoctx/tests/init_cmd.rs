//! End-to-end coverage for `repoctx init` (4b2af2a).
//!
//! HOME is pointed at a throwaway tempdir so the user-global conflict
//! scan stays hermetic, and `--rtk on|off` is passed explicitly so
//! RTK_CHAIN is deterministic regardless of whether rtk is on PATH.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run(repo: &Path, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .env("HOME", home)
        .arg("--repo")
        .arg(repo)
        .arg("init")
        .args(args)
        .assert()
}

fn bash_command(settings_path: &Path) -> String {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(settings_path).unwrap()).unwrap();
    v["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["matcher"] == "Bash")
        .expect("Bash matcher entry")["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn dry_run_writes_nothing() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(
        repo.path(),
        home.path(),
        &["--yes", "--rtk", "off", "--dry-run"],
    )
    .success();
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
    assert!(!repo.path().join(".claude/settings.json").exists());
}

#[test]
fn local_install_writes_script_settings_gitattributes_and_guidance() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();

    let script = repo.path().join(".repoctx/hook.sh");
    let body = std::fs::read_to_string(&script).unwrap();
    assert!(body.contains("# repoctx-hook-version: 1"));
    assert!(body.contains("RTK_CHAIN=1"));
    assert!(body.contains(r#"exec "$REPOCTX" hook claude --rtk-chain="$RTK_CHAIN""#));
    // (executable bit is set best-effort via `chmod`; not asserted here to
    // keep this suite free of OS-specific permission APIs — the e2e matrix
    // 0a338d7 verifies executability behaviorally.)

    assert_eq!(
        bash_command(&repo.path().join(".claude/settings.json")),
        ".repoctx/hook.sh"
    );

    let gitattrs = std::fs::read_to_string(repo.path().join(".gitattributes")).unwrap();
    assert!(gitattrs.contains("*.sh text eol=lf"));

    assert!(repo.path().join(".claude/skills/repoctx/SKILL.md").exists());
    let claude_md = std::fs::read_to_string(repo.path().join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("<!-- repoctx:start -->"));
}

#[test]
fn rtk_off_bakes_zero() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "off"]).success();
    let body = std::fs::read_to_string(repo.path().join(".repoctx/hook.sh")).unwrap();
    assert!(body.contains("RTK_CHAIN=0"));
}

#[test]
fn global_install_writes_to_home_with_absolute_path() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["-g", "--yes", "--rtk", "off"]).success();

    let script = home.path().join(".claude/repoctx-hook.sh");
    assert!(script.exists(), "global script written under HOME/.claude");

    let cmd = bash_command(&home.path().join(".claude/settings.json"));
    assert_eq!(
        cmd,
        script.display().to_string(),
        "global entry is absolute"
    );

    // No project files for a global install.
    assert!(!repo.path().join(".repoctx/hook.sh").exists());
    assert!(!repo.path().join("CLAUDE.md").exists());
}

#[test]
fn global_displaces_rtk_chains_it_and_backs_up() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // Pre-existing user-global rtk hook.
    let gclaude = home.path().join(".claude");
    std::fs::create_dir_all(&gclaude).unwrap();
    std::fs::write(
        gclaude.join("settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#,
    )
    .unwrap();

    // init -g over it: not refused (it's the recommended path), chains rtk.
    run(repo.path(), home.path(), &["-g", "--yes"]).success();

    // Sole owner now = the global script; rtk chained (RTK_CHAIN=1).
    let script = home.path().join(".claude/repoctx-hook.sh");
    assert_eq!(
        bash_command(&gclaude.join("settings.json")),
        script.display().to_string()
    );
    let body = std::fs::read_to_string(&script).unwrap();
    assert!(
        body.contains("RTK_CHAIN=1"),
        "rtk should be chained underneath"
    );

    // A backup of the prior settings.json was written.
    let has_backup = std::fs::read_dir(&gclaude)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains("repoctx-backup-"));
    assert!(has_backup, "expected a settings.json backup");
}

#[test]
fn idempotent_second_run() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();
    run(repo.path(), home.path(), &["--yes", "--rtk", "on"]).success();
    // Settings still has exactly one Bash entry pointing at the script.
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(repo.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let bash_entries = v["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["matcher"] == "Bash")
        .count();
    assert_eq!(bash_entries, 1);
}

#[test]
fn refuses_foreign_hook_unless_forced() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // Seed a project settings.json with an unrecognized Bash hook.
    let claude = repo.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"my-own-tool"}]}]}}"#,
    )
    .unwrap();

    // Without --force: refuse, naming the foreign command.
    let out = run(repo.path(), home.path(), &["--yes", "--rtk", "off"])
        .failure()
        .get_output()
        .stderr
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("my-own-tool"),
        "should name the foreign hook: {s}"
    );
    assert!(!repo.path().join(".repoctx/hook.sh").exists());

    // With --force: install anyway (takes over the Bash matcher).
    run(
        repo.path(),
        home.path(),
        &["--yes", "--rtk", "off", "--force"],
    )
    .success();
    assert_eq!(
        bash_command(&claude.join("settings.json")),
        ".repoctx/hook.sh"
    );
}

#[test]
fn migrates_v05x_chain_commands_to_script() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    // Seed a v0.5.x state: a hook.chain_commands row + an inline
    // `repoctx hook claude` settings entry.
    Command::cargo_bin("repoctx")
        .unwrap()
        .env("HOME", home.path())
        .arg("--repo")
        .arg(repo.path())
        .args(["config", "set", "hook.chain_commands", "rtk hook claude"])
        .assert()
        .success();
    let claude = repo.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"repoctx hook claude"}]}]}}"#,
    )
    .unwrap();

    // init migrates: ports the rtk chain, drops the row, rewrites the entry.
    let out = run(repo.path(), home.path(), &["--yes"])
        .success()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&out).contains("migrated"));

    let body = std::fs::read_to_string(repo.path().join(".repoctx/hook.sh")).unwrap();
    assert!(body.contains("RTK_CHAIN=1"), "rtk chain ported");
    assert_eq!(
        bash_command(&claude.join("settings.json")),
        ".repoctx/hook.sh"
    );

    // chain_commands row is gone (config get → default/empty).
    let get = Command::cargo_bin("repoctx")
        .unwrap()
        .env("HOME", home.path())
        .arg("--repo")
        .arg(repo.path())
        .args(["config", "get", "hook.chain_commands"])
        .output()
        .unwrap();
    let val = String::from_utf8_lossy(&get.stdout);
    assert!(
        !val.contains("rtk hook claude"),
        "chain_commands should be cleared, got: {val}"
    );
}

#[test]
fn unknown_agent_rejected() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let out = run(repo.path(), home.path(), &["--yes", "--agent", "aider"])
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&out).contains("unsupported agent 'aider'"));
}

#[test]
fn project_install_writes_skill_and_claude_md() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "off"]).success();
    let skill = repo.path().join(".claude/skills/repoctx/SKILL.md");
    assert!(skill.exists(), "project skill written");
    assert!(std::fs::read_to_string(&skill)
        .unwrap()
        .contains("callgraph"));
    // Project scope also writes the repo-root CLAUDE.md guidance block.
    assert!(repo.path().join("CLAUDE.md").exists());
}

#[test]
fn global_install_writes_skill_into_home_not_agents_md() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run(repo.path(), home.path(), &["--yes", "--rtk", "off", "-g"]).success();
    // Skill lands under ~/.claude/skills with the call-graph content.
    let skill = home.path().join(".claude/skills/repoctx/SKILL.md");
    assert!(skill.exists(), "global skill written to ~/.claude/skills");
    assert!(std::fs::read_to_string(&skill)
        .unwrap()
        .contains("callgraph"));
    // No stray repo-root guidance dumped into HOME.
    assert!(!home.path().join("AGENTS.md").exists(), "no ~/AGENTS.md");
    // And it must NOT touch the repo for a global install.
    assert!(!repo.path().join(".claude/skills/repoctx/SKILL.md").exists());
}
