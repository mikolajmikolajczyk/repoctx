//! `repoctx hook claude` — PreToolUse hook that intercepts agent
//! `rg`/`grep` invocations and routes them through repoctx when the
//! intent is "find a symbol by name". Unmatched commands chain to any
//! other hook the user had registered (typically `rtk hook claude`).
//!
//! Design: `wiki/decisions/2026-06-12-rewrite-hook-design.md`.

use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config::{HookConfig, HookUseRtk};

/// Pruning the Claude Code stdin payload to just the fields we touch.
#[derive(Debug, Deserialize)]
struct PreToolUseInput {
    tool_input: ToolInput,
}

#[derive(Debug, Deserialize)]
struct ToolInput {
    #[serde(default)]
    command: String,
}

/// Outgoing payload — same shape Claude Code expects.
#[derive(Debug, Serialize)]
struct PreToolUseOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "permissionDecision")]
    permission_decision: &'static str,
    #[serde(rename = "permissionDecisionReason")]
    permission_decision_reason: String,
    #[serde(rename = "updatedInput")]
    updated_input: UpdatedInput,
}

#[derive(Debug, Serialize)]
struct UpdatedInput {
    command: String,
}

/// One semantic rewrite. The matcher decides if it applies; the
/// builder produces the rewritten command string.
struct Rule {
    name: &'static str,
    matcher: fn(&[&str]) -> Option<RewriteIntent>,
    /// When the agent quotes the pattern (e.g. `rg "TODO"`) we treat
    /// it as an explicit literal-string match — passthrough so rtk's
    /// formatted grep handles it. Quoted *definition* patterns are
    /// the exception (`rg "fn parse_config"` is structural intent).
    skip_when_quoted: bool,
}

/// Result of a successful match: what the agent wanted + what
/// `repoctx` command answers it.
struct RewriteIntent {
    kind: &'static str, // "symbols" | "definition"
    ident: String,
}

impl RewriteIntent {
    fn to_command(&self) -> String {
        format!("repoctx {} {} --json", self.kind, self.ident)
    }
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn contains_shell_metacharacters(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '|' | '&' | ';' | '`' | '$' | '>' | '<' | '*' | '?' | '\\'
        )
    })
}

/// `rg` carrying any flag is unsafe to hand to the rtk chain: rtk's
/// `grep` wrapper forwards flags it doesn't recognize to GNU grep, which
/// loses ripgrep's recursive/gitignore defaults (silent empty results)
/// and rejects rg-only flags like `--type` / `-g` (hard error). Detect a
/// leading `rg ... -flag` so the caller can bypass the chain and let the
/// agent's real `rg` run untouched. Plain `rg PATTERN` (no flags) is
/// safe and still chains — rtk's formatted grep handles it.
///
/// Whitespace-split (quote-agnostic) and stop at the first shell control
/// operator so a flag inside `rg -i foo | head` is still caught.
fn is_flagged_rg(command: &str) -> bool {
    let mut words = command.split_whitespace();
    if words.next() != Some("rg") {
        return false;
    }
    for w in words {
        if matches!(w, "|" | "||" | "&&" | ";" | ">" | ">>" | "<" | "&") {
            break;
        }
        if w.starts_with('-') {
            return true;
        }
    }
    false
}

const RULES: &[Rule] = &[
    Rule {
        name: "rg <ident>",
        matcher: rg_single_ident,
        skip_when_quoted: true,
    },
    Rule {
        name: "rg \"fn <ident>\" / class / struct / function",
        matcher: rg_quoted_definition,
        skip_when_quoted: false,
    },
    Rule {
        name: "grep -r <ident> .",
        matcher: grep_r_single_ident,
        skip_when_quoted: true,
    },
    Rule {
        name: "grep -rn \"fn <ident>\" (and friends)",
        matcher: grep_rn_quoted_definition,
        skip_when_quoted: false,
    },
];

fn rg_single_ident(argv: &[&str]) -> Option<RewriteIntent> {
    // `rg <ident>` — exactly two tokens. No flags allowed.
    if argv.len() != 2 {
        return None;
    }
    if argv[0] != "rg" {
        return None;
    }
    let ident = argv[1];
    if !is_safe_identifier(ident) {
        return None;
    }
    Some(RewriteIntent {
        kind: "symbols",
        ident: ident.to_string(),
    })
}

fn rg_quoted_definition(argv: &[&str]) -> Option<RewriteIntent> {
    // `rg "fn foo"` / `rg "class Foo"` / `rg "struct Foo"` /
    // `rg "function foo"`. After tokenization the quoted string is
    // already a single argv entry.
    if argv.len() != 2 {
        return None;
    }
    if argv[0] != "rg" {
        return None;
    }
    parse_definition_pattern(argv[1])
}

fn grep_r_single_ident(argv: &[&str]) -> Option<RewriteIntent> {
    // `grep -r <ident> .`  (also `-R`)
    if argv.len() != 4 {
        return None;
    }
    if argv[0] != "grep" {
        return None;
    }
    if !matches!(argv[1], "-r" | "-R") {
        return None;
    }
    if argv[3] != "." {
        return None;
    }
    let ident = argv[2];
    if !is_safe_identifier(ident) {
        return None;
    }
    Some(RewriteIntent {
        kind: "symbols",
        ident: ident.to_string(),
    })
}

fn grep_rn_quoted_definition(argv: &[&str]) -> Option<RewriteIntent> {
    // `grep -rn "fn foo" .` and family.
    if argv.len() != 4 {
        return None;
    }
    if argv[0] != "grep" {
        return None;
    }
    if !matches!(argv[1], "-rn" | "-nr" | "-Rn" | "-nR") {
        return None;
    }
    if argv[3] != "." {
        return None;
    }
    parse_definition_pattern(argv[2])
}

fn parse_definition_pattern(pat: &str) -> Option<RewriteIntent> {
    // `fn <ident>` / `class <ident>` / `struct <ident>` /
    // `function <ident>`.
    let tokens: Vec<&str> = pat.split_whitespace().collect();
    if tokens.len() != 2 {
        return None;
    }
    if !matches!(tokens[0], "fn" | "class" | "struct" | "function") {
        return None;
    }
    let ident = tokens[1];
    if !is_safe_identifier(ident) {
        return None;
    }
    Some(RewriteIntent {
        kind: "definition",
        ident: ident.to_string(),
    })
}

/// Cheap shell tokenizer: splits on whitespace, respects double
/// quotes. Refuses to tokenize anything with shell metacharacters
/// (returns `None` so the caller falls through to passthrough).
fn tokenize(cmd: &str) -> Option<Vec<String>> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    if contains_shell_metacharacters(trimmed) {
        return None;
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_dquote = false;
    for c in trimmed.chars() {
        match c {
            '"' if !in_dquote => in_dquote = true,
            '"' if in_dquote => {
                in_dquote = false;
                out.push(std::mem::take(&mut cur));
            }
            c if c.is_whitespace() && !in_dquote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if in_dquote {
        return None; // unterminated quote
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Some(out)
}

pub(crate) fn try_semantic_rewrite(command: &str) -> Option<(String, &'static str)> {
    let tokens = tokenize(command)?;
    let argv: Vec<&str> = tokens.iter().map(String::as_str).collect();
    // The single-ident rules treat quoted arguments as the agent's
    // explicit signal "this is a literal string match, leave it
    // alone". They route to the chain (rtk's formatted grep / rg).
    let any_quotes = command.contains('"');
    for rule in RULES {
        if rule.skip_when_quoted && any_quotes {
            continue;
        }
        if let Some(intent) = (rule.matcher)(&argv) {
            return Some((intent.to_command(), rule.name));
        }
    }
    None
}

/// Drive one chain command with the same stdin payload we received.
/// Returns Ok(Some(stdout)) on rewrite (we propagate verbatim),
/// Ok(None) on passthrough (try next chain or exit 1), Err on
/// transport failure (logged + treated as passthrough).
fn exec_chain(cmd: &str, stdin_bytes: &[u8]) -> Result<Option<String>> {
    let parts = tokenize(cmd)
        .filter(|t| !t.is_empty())
        .context("chain command failed to tokenize")?;
    let mut iter = parts.into_iter();
    let program = iter.next().context("chain command is empty")?;
    let mut child = Command::new(&program)
        .args(iter)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn chain {program}"))?;
    if let Some(mut child_stdin) = child.stdin.take() {
        use std::io::Write;
        child_stdin
            .write_all(stdin_bytes)
            .context("write to chain stdin")?;
    }
    let output = child.wait_with_output().context("await chain exit")?;
    match output.status.code() {
        Some(0) => {
            let stdout = String::from_utf8(output.stdout).context("chain stdout is not UTF-8")?;
            Ok(Some(stdout))
        }
        Some(1) | None => Ok(None),
        Some(2) => {
            // Deny — propagate verbatim by emitting the chain's stdout
            // (Claude Code expects a payload, not just exit code 2 —
            // rtk produces one). We use exit 2 below.
            let stdout = String::from_utf8(output.stdout).unwrap_or_default();
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            std::process::exit(2);
        }
        Some(other) => {
            warn!(program = %program, exit = other, "chain command failed unexpectedly; treating as passthrough");
            Ok(None)
        }
    }
}

/// Top-level entry. Returns the JSON payload to print on rewrite, or
/// `Ok(None)` for silent passthrough.
fn run_inner(stdin_bytes: &[u8], cfg: &HookConfig, rtk_chain: bool) -> Result<Option<String>> {
    let payload: PreToolUseInput =
        serde_json::from_slice(stdin_bytes).context("parse PreToolUse stdin")?;
    let command = payload.tool_input.command.trim();
    if command.is_empty() {
        return Ok(None);
    }

    // Semantic rewrite (skipped when hook.rewrite = off).
    use crate::config::HookRewrite;
    if matches!(cfg.rewrite, HookRewrite::Auto | HookRewrite::Force) {
        if let Some((rewritten, rule_name)) = try_semantic_rewrite(command) {
            debug!(rule = rule_name, %command, %rewritten, "semantic rewrite");
            let reply = PreToolUseOutput {
                hook_specific_output: HookSpecificOutput {
                    hook_event_name: "PreToolUse",
                    permission_decision: "allow",
                    permission_decision_reason: format!(
                        "repoctx rewrote ({rule_name}): {command} → {rewritten}"
                    ),
                    updated_input: UpdatedInput { command: rewritten },
                },
            };
            return Ok(Some(serde_json::to_string(&reply)?));
        }
    }

    // Flagged `rg` is unsafe to chain (rtk degrades it to GNU grep —
    // see is_flagged_rg). Bypass every chain so the real ripgrep runs.
    if is_flagged_rg(command) {
        debug!(%command, "flagged rg — bypassing chain so real ripgrep runs");
        return Ok(None);
    }

    // Legacy chain dispatch (v0.5.x `hook.chain_commands`). Fresh
    // script-based installs leave this empty and rely on the rtk chain
    // below instead.
    for chain in &cfg.chain_commands {
        match exec_chain(chain, stdin_bytes) {
            Ok(Some(stdout)) => {
                debug!(%chain, "chain handled the rewrite");
                return Ok(Some(stdout));
            }
            Ok(None) => continue,
            Err(e) => {
                warn!(%chain, error = %e, "chain dispatch failed; trying next");
                continue;
            }
        }
    }

    // Chain: on passthrough, hand off to the first allowlisted tool on
    // PATH (`hook.chainable`, rtk by default) so its output compression
    // still applies. The script bakes `--rtk-chain`; a direct invocation
    // resolves it from `hook.use_rtk`.
    if rtk_chain {
        let mut any_present = false;
        for tool in &cfg.chainable {
            if which(tool).is_none() {
                continue;
            }
            any_present = true;
            match exec_chain(&format!("{tool} hook claude"), stdin_bytes) {
                Ok(Some(stdout)) => {
                    debug!(%tool, "chain handled the rewrite");
                    return Ok(Some(stdout));
                }
                Ok(None) => {}
                Err(e) => warn!(%tool, error = %e, "chain failed; trying next"),
            }
        }
        if !any_present {
            warn_once_chain_missing(&cfg.chainable);
        }
    }

    Ok(None)
}

/// CLI entry point. Reads stdin, writes JSON to stdout on rewrite,
/// exits 0 on rewrite, exits 1 on passthrough, never panics on
/// malformed input (logs + passthrough).
///
/// `rtk_chain_flag` is the resolved `--rtk-chain` value; `None` falls
/// back to `hook.use_rtk` (`on`/`off`/`auto`, where `auto` = rtk on PATH).
pub fn run(cfg: &HookConfig, rtk_chain_flag: Option<bool>) -> Result<i32> {
    let mut stdin_bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut stdin_bytes)
        .context("read stdin")?;
    let rtk_chain = rtk_chain_flag.unwrap_or(match cfg.use_rtk {
        HookUseRtk::On => true,
        HookUseRtk::Off => false,
        HookUseRtk::Auto => cfg.chainable.iter().any(|t| which(t).is_some()),
    });
    match run_inner(&stdin_bytes, cfg, rtk_chain) {
        Ok(Some(json)) => {
            println!("{json}");
            Ok(0)
        }
        Ok(None) => Ok(1),
        Err(e) => {
            warn!(error = %e, "hook claude: internal error; passthrough");
            Ok(1)
        }
    }
}

/// Locate a program on `PATH` without spawning it (cheap; the hook runs
/// on every Bash tool call).
pub(crate) fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let cand = dir.join(format!("{prog}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Warn once (per cache dir) that chaining is on but no allowlisted tool
/// is on PATH.
fn warn_once_chain_missing(chainable: &[String]) {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    let msg = format!(
        "[repoctx] hook chaining enabled but none of [{}] found on PATH — \
         install one or set hook.use_rtk=off (https://github.com/rtk-ai/rtk)",
        chainable.join(", ")
    );
    let Some(dir) = dir else {
        eprintln!("{msg}");
        return;
    };
    let sentinel = dir.join("repoctx-rtk-missing-warned");
    if sentinel.exists() {
        return;
    }
    eprintln!("{msg}");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&sentinel, b"");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple() {
        assert_eq!(
            tokenize("rg foo").unwrap(),
            vec!["rg".to_string(), "foo".to_string()]
        );
    }

    #[test]
    fn tokenize_quoted() {
        assert_eq!(
            tokenize(r#"rg "fn foo""#).unwrap(),
            vec!["rg".to_string(), "fn foo".to_string()]
        );
    }

    #[test]
    fn tokenize_refuses_shell_metas() {
        assert!(tokenize("rg foo | head").is_none());
        assert!(tokenize("rg $FOO").is_none());
        assert!(tokenize("rg `foo`").is_none());
        assert!(tokenize("rg foo > out").is_none());
    }

    #[test]
    fn rg_single_ident_matches() {
        let (cmd, _) = try_semantic_rewrite("rg parseConfig").unwrap();
        assert_eq!(cmd, "repoctx symbols parseConfig --json");
    }

    #[test]
    fn rg_with_flags_passthrough() {
        assert!(try_semantic_rewrite("rg -n foo").is_none());
        assert!(try_semantic_rewrite("rg -l foo").is_none());
        assert!(try_semantic_rewrite("rg --json foo").is_none());
    }

    #[test]
    fn rg_with_regex_passthrough() {
        assert!(try_semantic_rewrite("rg foo.*").is_none());
        assert!(try_semantic_rewrite("rg ^foo").is_none());
        assert!(try_semantic_rewrite("rg foo|bar").is_none());
    }

    #[test]
    fn rg_with_multiple_idents_passthrough() {
        assert!(try_semantic_rewrite("rg foo bar").is_none());
    }

    #[test]
    fn rg_quoted_definition_pattern() {
        let (cmd, _) = try_semantic_rewrite(r#"rg "fn parse_config""#).unwrap();
        assert_eq!(cmd, "repoctx definition parse_config --json");
        let (cmd, _) = try_semantic_rewrite(r#"rg "class UserService""#).unwrap();
        assert_eq!(cmd, "repoctx definition UserService --json");
        let (cmd, _) = try_semantic_rewrite(r#"rg "struct Cat""#).unwrap();
        assert_eq!(cmd, "repoctx definition Cat --json");
    }

    #[test]
    fn rg_random_quoted_pattern_passthrough() {
        assert!(try_semantic_rewrite(r#"rg "TODO""#).is_none());
        assert!(try_semantic_rewrite(r#"rg "important""#).is_none());
        assert!(try_semantic_rewrite(r#"rg "fn""#).is_none());
    }

    #[test]
    fn grep_r_single_ident_matches() {
        let (cmd, _) = try_semantic_rewrite("grep -r Editor .").unwrap();
        assert_eq!(cmd, "repoctx symbols Editor --json");
        let (cmd, _) = try_semantic_rewrite("grep -R Editor .").unwrap();
        assert_eq!(cmd, "repoctx symbols Editor --json");
    }

    #[test]
    fn grep_with_extra_paths_passthrough() {
        assert!(try_semantic_rewrite("grep -r Editor src/").is_none());
        assert!(try_semantic_rewrite("grep -r Editor").is_none());
    }

    #[test]
    fn grep_rn_quoted_definition() {
        let (cmd, _) = try_semantic_rewrite(r#"grep -rn "fn main" ."#).unwrap();
        assert_eq!(cmd, "repoctx definition main --json");
    }

    #[test]
    fn empty_command_passthrough() {
        assert!(try_semantic_rewrite("").is_none());
        assert!(try_semantic_rewrite("   ").is_none());
    }

    #[test]
    fn unknown_command_passthrough() {
        assert!(try_semantic_rewrite("ls -la").is_none());
        assert!(try_semantic_rewrite("git status").is_none());
        assert!(try_semantic_rewrite("cat README.md").is_none());
    }

    #[test]
    fn flagged_rg_bypasses_chain() {
        // Flagged rg → bypass (rtk would degrade it to GNU grep).
        assert!(is_flagged_rg("rg -i Foo"));
        assert!(is_flagged_rg("rg --type rust Foo"));
        assert!(is_flagged_rg("rg -g '*.rs' Foo"));
        assert!(is_flagged_rg("rg -n Foo"));
        // Flag before a pipe is still caught.
        assert!(is_flagged_rg("rg -i Foo | head"));
    }

    #[test]
    fn unflagged_rg_still_chains() {
        // Plain rg (rtk handles it) and non-rg commands are not bypassed.
        assert!(!is_flagged_rg("rg Foo"));
        assert!(!is_flagged_rg("rg Foo | head"));
        assert!(!is_flagged_rg(r#"rg "fn foo""#));
        assert!(!is_flagged_rg("grep -r Foo ."));
        assert!(!is_flagged_rg("git status"));
        assert!(!is_flagged_rg(""));
    }

    // ── Rewrite-decision corpus (issue 573eccc) ──────────────────────
    //
    // Drives the *pure* decision function over a shared data file. The
    // same corpus is run through the `repoctx hook claude` CLI in
    // `tests/rewrite_corpus.rs`; the two must agree.

    #[derive(serde::Deserialize)]
    struct Corpus {
        case: Vec<Case>,
    }

    #[derive(serde::Deserialize)]
    struct Case {
        cmd: String,
        expect: String,
        #[serde(default)]
        rule: String,
        #[serde(default)]
        to: Option<String>,
    }

    fn load_corpus() -> Vec<Case> {
        let text = include_str!("../tests/fixtures/rewrite_corpus.toml");
        let c: Corpus = toml::from_str(text).expect("corpus parses");
        c.case
    }

    #[test]
    fn corpus_pure_decisions_match() {
        let cases = load_corpus();
        assert!(
            cases.len() >= 100,
            "corpus has {} rows, expected >= 100",
            cases.len()
        );
        for c in &cases {
            let got = try_semantic_rewrite(&c.cmd);
            match c.expect.as_str() {
                "rewrite" => {
                    let (cmd, rule) = got.unwrap_or_else(|| {
                        panic!("expected REWRITE for `{}`, got passthrough", c.cmd)
                    });
                    let want =
                        c.to.as_deref()
                            .unwrap_or_else(|| panic!("rewrite row `{}` missing `to`", c.cmd));
                    assert_eq!(cmd, want, "rewritten command mismatch for `{}`", c.cmd);
                    assert_eq!(rule, c.rule, "rule mismatch for `{}`", c.cmd);
                }
                "passthrough" => {
                    assert!(
                        got.is_none(),
                        "expected PASSTHROUGH for `{}`, got rewrite {:?}",
                        c.cmd,
                        got.map(|(cmd, _)| cmd)
                    );
                }
                other => panic!("bad `expect` value `{other}` for `{}`", c.cmd),
            }
        }
    }

    #[test]
    fn corpus_covers_every_rule_at_least_five_times() {
        use std::collections::HashMap;
        let cases = load_corpus();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for c in &cases {
            if c.expect == "rewrite" {
                *counts.entry(c.rule.as_str()).or_default() += 1;
            }
        }
        for rule in RULES {
            let n = counts.get(rule.name).copied().unwrap_or(0);
            assert!(
                n >= 5,
                "rule `{}` is under-covered ({n} rows, need >= 5)",
                rule.name
            );
        }
        // Every rule named in the corpus must be a real rule (typo guard).
        let known: Vec<&str> = RULES.iter().map(|r| r.name).collect();
        for r in counts.keys() {
            assert!(known.contains(r), "corpus references unknown rule `{r}`");
        }
    }
}
