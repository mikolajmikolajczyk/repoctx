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
    kind: &'static str, // "search" | "definition" | "context"
    ident: String,
    /// `--lang <slug>` for `search` (from rg `--type`). None = all langs.
    lang: Option<String>,
    /// Context window for `context` (from rg `-A/-B/-C`). Only set when
    /// `kind == "context"`.
    context: Option<u32>,
}

impl RewriteIntent {
    /// Ambiguous-intent searches route to `repoctx search` (textually
    /// complete: symbol defs + every ripgrep match), NOT `repoctx symbols`
    /// (which silently drops comment/string/non-symbol matches). See epic
    /// f4cb992.
    fn search(ident: impl Into<String>) -> Self {
        Self {
            kind: "search",
            ident: ident.into(),
            lang: None,
            context: None,
        }
    }

    fn definition(ident: impl Into<String>) -> Self {
        Self {
            kind: "definition",
            ident: ident.into(),
            lang: None,
            context: None,
        }
    }

    fn context(ident: impl Into<String>, lines: u32) -> Self {
        Self {
            kind: "context",
            ident: ident.into(),
            lang: None,
            context: Some(lines),
        }
    }

    fn with_lang(mut self, lang: Option<String>) -> Self {
        self.lang = lang;
        self
    }

    fn to_command(&self) -> String {
        if self.kind == "context" {
            let n = self.context.unwrap_or(5);
            return format!("repoctx context {} --context {n} --json", self.ident);
        }
        let mut s = format!("repoctx {} {} --json", self.kind, self.ident);
        if let Some(lang) = &self.lang {
            s.push_str(&format!(" --lang {lang}"));
        }
        s
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

/// A command the rtk chain handles *unfaithfully* — bypass the chain so the
/// agent's real tool runs untouched. We own chain correctness regardless of
/// fault, so any command rtk silently corrupts gets added here rather than
/// filed upstream. This is a denylist: it stays correct across rtk versions
/// (a bypass only ever forfeits compression, never correctness), but it does
/// not auto-detect *new* rtk regressions — re-run the fidelity audit on rtk
/// bumps. Current entry:
///
/// - **flagged `rg` (`-i` / `--type` / `-g` / …):** rtk forwards rg-only
///   flags to GNU grep, losing ripgrep's recursive/gitignore defaults
///   (silent empty) or erroring outright. Still broken as of rtk 0.42.4.
///
/// (`ls` lived here too — rtk ≤0.41 returned `(empty)` for any directory —
/// but rtk 0.42.4 fixed it, so it is back to chaining; restore the bypass if
/// you must support older rtk.)
///
/// Commands rtk *compresses faithfully* stay chained: plain `rg PATTERN`,
/// `grep`, `cat`, `wc`, `git`, `ls`, `tail`, and the lossy-but-**signaled**
/// ones (`find` prints "+N more", `head`/`tree` print a truncation marker) —
/// the agent is never misled there, and that compression is the whole point.
///
/// Detection runs per pipeline/list segment. Splitting is quote-agnostic, so
/// a `;`/`|` inside a quoted argument can over-split into a false positive —
/// harmless: we only ever lose rtk compression on that compound, never run
/// the wrong command.
fn is_chain_unsafe(command: &str) -> bool {
    command
        .split(['|', '&', ';', '\n'])
        .any(segment_is_chain_unsafe)
}

/// True when a single command segment is one rtk corrupts. Caller splits the
/// full command on control operators. One predicate today (flagged `rg`); add
/// more `|| segment_is_<x>` as the fidelity canary surfaces them.
fn segment_is_chain_unsafe(segment: &str) -> bool {
    segment_is_flagged_rg(segment)
}

/// True when a segment is `rg` followed by a flag, before any redirection.
fn segment_is_flagged_rg(segment: &str) -> bool {
    let mut words = segment.split_whitespace();
    if words.next() != Some("rg") {
        return false;
    }
    for w in words {
        if matches!(w, ">" | ">>" | "<") {
            break;
        }
        if w.starts_with('-') {
            return true;
        }
    }
    false
}

// ── Passthrough telemetry (issue #7) ───────────────────────────────────
//
// Classify a grep/rg/find command into a coarse idiom bucket so `repoctx
// discover` can rank how often each shape is rewritten vs leaks to grep.
// Heuristic and deliberately cheap; refined from the data it collects.
// Returns None for non-grep-family commands so the hot hook path skips the
// store entirely for everything else.

/// `(tool, idiom)` for the first grep-family / find segment in `command`.
pub(crate) fn classify_idiom(command: &str) -> Option<(&'static str, &'static str)> {
    // Precise path for simple commands (handles quotes); rough segment scan
    // for compounds (pipes, loops, redirects) that `tokenize` rejects.
    if let Some(tokens) = tokenize(command) {
        if let Some((tool, rest)) = split_tool(&tokens) {
            return Some((tool, idiom_for(tool, rest)));
        }
        return None;
    }
    for seg in command.split(['|', '&', ';', '\n']) {
        let words: Vec<&str> = seg.split_whitespace().collect();
        if let Some((tool, rest)) = split_tool_str(&words) {
            return Some((tool, idiom_for(tool, rest)));
        }
    }
    None
}

fn tool_slug(w0: &str) -> Option<&'static str> {
    match w0 {
        "rg" => Some("rg"),
        "grep" | "egrep" | "fgrep" => Some("grep"),
        "find" => Some("find"),
        "ag" | "ack" => Some("grep"),
        _ => None,
    }
}

fn split_tool(tokens: &[String]) -> Option<(&'static str, &[String])> {
    let w0 = tokens.first()?;
    let tool = tool_slug(w0)?;
    Some((tool, &tokens[1..]))
}

fn split_tool_str<'a>(words: &'a [&'a str]) -> Option<(&'static str, &'a [&'a str])> {
    let w0 = words.first()?;
    let tool = tool_slug(w0)?;
    Some((tool, &words[1..]))
}

/// Idiom bucket from a tool's argument list (everything after the program).
/// Generic over `&str`-like args so it serves both the tokenized and rough
/// paths.
fn idiom_for<S: AsRef<str>>(tool: &str, args: &[S]) -> &'static str {
    if tool == "find" {
        return "find";
    }
    // Split flags from positionals (pattern + paths). `--` ends flags.
    let mut nav_flag = false;
    let mut positionals: Vec<&str> = Vec::new();
    let mut flags_done = false;
    for a in args {
        let a = a.as_ref();
        if !flags_done && a == "--" {
            flags_done = true;
            continue;
        }
        if !flags_done && a.starts_with('-') && a.len() > 1 {
            if is_nav_flag(a) {
                nav_flag = true;
            }
            continue;
        }
        positionals.push(a);
    }
    let Some(pattern) = positionals.first().copied() else {
        return "other";
    };
    let has_explicit_path = positionals.iter().skip(1).any(|p| *p != ".");

    if pattern.contains('|') {
        return "multi-term";
    }
    let lower = pattern.to_ascii_lowercase();
    if ["import", "require", "from ", "from(", "use "]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        return "import-shape";
    }
    if pattern.contains('(') {
        return "call-shape";
    }
    if pattern.contains(|c| {
        matches!(
            c,
            '.' | '*' | '+' | '?' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
        )
    }) {
        return "regex";
    }
    if has_explicit_path {
        return "explicit-path";
    }
    if is_safe_identifier(pattern) {
        return if nav_flag {
            "flagged-nav-ident"
        } else {
            "bare-ident"
        };
    }
    "other"
}

/// Navigation flags (format/scope, not result-set-changing). Mirrors the
/// rewrite rules' notion so telemetry buckets line up with what we rewrite.
fn is_nav_flag(flag: &str) -> bool {
    matches!(
        flag,
        "-n" | "-l"
            | "-i"
            | "-w"
            | "-F"
            | "--type"
            | "-A"
            | "-B"
            | "-C"
            | "-r"
            | "-R"
            | "-rn"
            | "-nr"
            | "-Rn"
            | "-nR"
    ) || flag.starts_with("--type")
}

/// Best-effort telemetry write. No-op unless an index DB already exists (so
/// we never create `.repoctx/` just to log) and `enabled`. Errors swallowed —
/// telemetry must never affect the command.
fn record_event(
    repo_root: &std::path::Path,
    enabled: bool,
    idiom: &Option<(&'static str, &'static str)>,
    outcome: &str,
) {
    if !enabled {
        return;
    }
    let Some((tool, id)) = idiom else { return };
    if !repo_root.join(".repoctx/index.db").exists() {
        return;
    }
    if let Ok(mut store) = repoctx_store::Store::open(repo_root) {
        let _ = store.record_hook_event(tool, id, outcome);
    }
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
        name: "rg [flags] <ident> (navigation flags → search/context)",
        matcher: rg_flagged_ident,
        skip_when_quoted: true,
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
    Some(RewriteIntent::search(ident))
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
    Some(RewriteIntent::search(ident))
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
    Some(RewriteIntent::definition(ident))
}

/// `rg [navigation-flags] <ident>` — a flagged search whose flags change
/// rg's *output* but not the *intent* (locate `<ident>`). Rewrites to
/// `repoctx search`/`context` so the agent's habitual flagged `rg` still
/// gets repoctx instead of bypassing to ripgrep. Only a curated allowlist of
/// flags is accepted; anything else (regex, paths, `-v`/`-o`/`-c`, multiple
/// positionals) returns None → the command bypasses to real ripgrep.
fn rg_flagged_ident(argv: &[&str]) -> Option<RewriteIntent> {
    if argv.first() != Some(&"rg") {
        return None;
    }
    let mut lang: Option<String> = None;
    let mut context: Option<u32> = None;
    let mut idents: Vec<&str> = Vec::new();
    let mut saw_flag = false;

    let mut i = 1;
    while i < argv.len() {
        let a = argv[i];
        // --type <t> / -t <t> / --type=<t>  → --lang
        if a == "--type" || a == "-t" {
            saw_flag = true;
            i += 1;
            lang = Some(map_rg_type(argv.get(i)?)?);
        } else if let Some(t) = a.strip_prefix("--type=") {
            saw_flag = true;
            lang = Some(map_rg_type(t)?);
        }
        // context: -A/-B/-C <n> / --context <n> / -C<n> / --context=<n>
        else if a == "-A" || a == "-B" || a == "-C" || a == "--context" {
            saw_flag = true;
            i += 1;
            let n: u32 = argv.get(i)?.parse().ok()?;
            context = Some(context.map_or(n, |c| c.max(n)));
        } else if let Some(n) = a
            .strip_prefix("--context=")
            .or_else(|| a.strip_prefix("-A"))
            .or_else(|| a.strip_prefix("-B"))
            .or_else(|| a.strip_prefix("-C"))
            .filter(|s| !s.is_empty())
        {
            saw_flag = true;
            let n: u32 = n.parse().ok()?;
            context = Some(context.map_or(n, |c| c.max(n)));
        }
        // long boolean nav flags
        else if a.starts_with("--") {
            match a {
                "--files-with-matches"
                | "--ignore-case"
                | "--word-regexp"
                | "--line-number"
                | "--smart-case"
                | "--case-sensitive"
                | "--fixed-strings" => saw_flag = true,
                _ => return None,
            }
        }
        // short boolean nav flags, possibly bundled (-in, -il, …)
        else if let Some(rest) = a.strip_prefix('-').filter(|_| a.len() >= 2) {
            if !rest
                .chars()
                .all(|c| matches!(c, 'i' | 'n' | 'l' | 'w' | 's' | 'S' | 'F'))
            {
                return None;
            }
            saw_flag = true;
        } else {
            idents.push(a);
        }
        i += 1;
    }

    // Exactly one bare-identifier positional, and at least one flag (the
    // no-flag case is `rg_single_ident`'s job).
    if !saw_flag || idents.len() != 1 || !is_safe_identifier(idents[0]) {
        return None;
    }
    let ident = idents[0];
    match context {
        Some(n) => Some(RewriteIntent::context(ident, n)),
        None => Some(RewriteIntent::search(ident).with_lang(lang)),
    }
}

/// Map a ripgrep `--type` name to a repoctx language slug. Unknown types
/// return None so the command bypasses rather than rewriting with a wrong
/// `--lang`.
fn map_rg_type(t: &str) -> Option<String> {
    let slug = match t {
        "rust" => "rust",
        "go" => "go",
        "py" | "python" => "python",
        "js" | "javascript" => "javascript",
        "ts" | "typescript" => "typescript",
        "c" => "c",
        "cpp" | "cxx" | "cc" => "cpp",
        "java" => "java",
        "ruby" | "rb" => "ruby",
        "csharp" | "cs" => "csharp",
        "php" => "php",
        "lua" => "lua",
        "kotlin" | "kt" => "kotlin",
        "swift" => "swift",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "sh" | "bash" => "bash",
        _ => return None,
    };
    Some(slug.to_string())
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
fn run_inner(
    stdin_bytes: &[u8],
    repo_root: &std::path::Path,
    cfg: &HookConfig,
    rtk_chain: bool,
) -> Result<Option<String>> {
    let payload: PreToolUseInput =
        serde_json::from_slice(stdin_bytes).context("parse PreToolUse stdin")?;
    let command = payload.tool_input.command.trim();
    if command.is_empty() {
        return Ok(None);
    }

    // Classify once for telemetry (issue #7). `None` for non-grep-family
    // commands, which skip the store entirely.
    let idiom = if cfg.telemetry {
        classify_idiom(command)
    } else {
        None
    };

    // Semantic rewrite (skipped when hook.rewrite = off).
    use crate::config::HookRewrite;
    if matches!(cfg.rewrite, HookRewrite::Auto | HookRewrite::Force) {
        if let Some((rewritten, rule_name)) = try_semantic_rewrite(command) {
            debug!(rule = rule_name, %command, %rewritten, "semantic rewrite");
            record_event(repo_root, cfg.telemetry, &idiom, "rewritten");
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

    // Commands rtk corrupts (broken `ls`, flagged `rg` → GNU grep — see
    // is_chain_unsafe) bypass every chain so the agent's real tool runs.
    if is_chain_unsafe(command) {
        debug!(%command, "chain-unsafe command — bypassing chain so the real tool runs");
        record_event(repo_root, cfg.telemetry, &idiom, "passthrough");
        return Ok(None);
    }

    // Legacy chain dispatch (v0.5.x `hook.chain_commands`). Fresh
    // script-based installs leave this empty and rely on the rtk chain
    // below instead.
    for chain in &cfg.chain_commands {
        match exec_chain(chain, stdin_bytes) {
            Ok(Some(stdout)) => {
                debug!(%chain, "chain handled the rewrite");
                record_event(repo_root, cfg.telemetry, &idiom, "chained");
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
                    record_event(repo_root, cfg.telemetry, &idiom, "chained");
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

    record_event(repo_root, cfg.telemetry, &idiom, "passthrough");
    Ok(None)
}

/// CLI entry point. Reads stdin, writes JSON to stdout on rewrite,
/// exits 0 on rewrite, exits 1 on passthrough, never panics on
/// malformed input (logs + passthrough).
///
/// `rtk_chain_flag` is the resolved `--rtk-chain` value; `None` falls
/// back to `hook.use_rtk` (`on`/`off`/`auto`, where `auto` = rtk on PATH).
pub fn run(
    repo_root: &std::path::Path,
    cfg: &HookConfig,
    rtk_chain_flag: Option<bool>,
) -> Result<i32> {
    let mut stdin_bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut stdin_bytes)
        .context("read stdin")?;
    let rtk_chain = rtk_chain_flag.unwrap_or(match cfg.use_rtk {
        HookUseRtk::On => true,
        HookUseRtk::Off => false,
        HookUseRtk::Auto => cfg.chainable.iter().any(|t| which(t).is_some()),
    });
    match run_inner(&stdin_bytes, repo_root, cfg, rtk_chain) {
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
    fn classify_idiom_buckets() {
        let c = |cmd| classify_idiom(cmd);
        assert_eq!(c("rg foo"), Some(("rg", "bare-ident")));
        assert_eq!(c("rg -n foo"), Some(("rg", "flagged-nav-ident")));
        assert_eq!(c("grep -rn foo ."), Some(("grep", "flagged-nav-ident")));
        assert_eq!(c("rg foo src/x.rs"), Some(("rg", "explicit-path")));
        assert_eq!(c("rg foo.*bar"), Some(("rg", "regex")));
        assert_eq!(c(r#"rg "foo\(""#), Some(("rg", "call-shape")));
        assert_eq!(c("rg import"), Some(("rg", "import-shape")));
        assert_eq!(c("find . -name x"), Some(("find", "find")));
        // Not a grep-family command -> no telemetry.
        assert_eq!(c("cargo build"), None);
        assert_eq!(c("ls -la"), None);
    }

    #[test]
    fn classify_idiom_compound_finds_grep_segment() {
        // Pipes/compounds: tokenize rejects, rough scan still finds the grep.
        assert_eq!(
            classify_idiom("cat x | grep foo"),
            Some(("grep", "bare-ident"))
        );
        // Alternation in an unquoted pattern splits on `|`; first rg segment
        // is what we bucket.
        assert!(classify_idiom("rg -nE aaa|bbb src/x.ts").is_some());
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
        assert_eq!(cmd, "repoctx search parseConfig --json");
    }

    #[test]
    fn rg_navigation_flags_rewrite_to_symbols() {
        // Output-shaping flags don't change the "locate <ident>" intent.
        for cmd in [
            "rg -n foo",
            "rg -l foo",
            "rg -i foo",
            "rg -w foo",
            "rg -in foo",
        ] {
            let (got, _) = try_semantic_rewrite(cmd).unwrap_or_else(|| panic!("{cmd}"));
            assert_eq!(got, "repoctx search foo --json", "{cmd}");
        }
    }

    #[test]
    fn rg_type_flag_maps_to_lang() {
        let (got, _) = try_semantic_rewrite("rg --type rust foo").unwrap();
        assert_eq!(got, "repoctx search foo --json --lang rust");
        let (got, _) = try_semantic_rewrite("rg -t py foo").unwrap();
        assert_eq!(got, "repoctx search foo --json --lang python");
    }

    #[test]
    fn rg_context_flags_map_to_context_command() {
        let (got, _) = try_semantic_rewrite("rg -C 3 foo").unwrap();
        assert_eq!(got, "repoctx context foo --context 3 --json");
        let (got, _) = try_semantic_rewrite("rg -C5 foo").unwrap();
        assert_eq!(got, "repoctx context foo --context 5 --json");
    }

    #[test]
    fn rg_non_navigation_flags_passthrough() {
        // Textual / output intents repoctx can't honor stay passthrough.
        assert!(try_semantic_rewrite("rg --json foo").is_none());
        assert!(try_semantic_rewrite("rg -c foo").is_none());
        assert!(try_semantic_rewrite("rg -v foo").is_none());
        assert!(try_semantic_rewrite("rg -o foo").is_none());
        // Unknown --type bails rather than guessing a wrong --lang.
        assert!(try_semantic_rewrite("rg --type cobol foo").is_none());
        // A real path/regex arg is not a bare identifier.
        assert!(try_semantic_rewrite("rg -n foo src/").is_none());
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
        assert_eq!(cmd, "repoctx search Editor --json");
        let (cmd, _) = try_semantic_rewrite("grep -R Editor .").unwrap();
        assert_eq!(cmd, "repoctx search Editor --json");
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
        assert!(is_chain_unsafe("rg -i Foo"));
        assert!(is_chain_unsafe("rg --type rust Foo"));
        assert!(is_chain_unsafe("rg -g '*.rs' Foo"));
        assert!(is_chain_unsafe("rg -n Foo"));
        // Flag before a pipe is still caught.
        assert!(is_chain_unsafe("rg -i Foo | head"));
        // Flagged rg in a *later* segment is caught too (the Option-1 fix).
        assert!(is_chain_unsafe("cat f.log | rg -i Foo"));
        assert!(is_chain_unsafe("cmd && rg --type rust Foo"));
        assert!(is_chain_unsafe("echo hi; rg -g '*.rs' Foo"));
        assert!(is_chain_unsafe("a | b | rg -n Foo | head"));
        // Flagged rg with a redirect is still caught (flag precedes it).
        assert!(is_chain_unsafe("rg -i Foo > out.txt"));
        assert!(is_chain_unsafe("rg -i Foo 2>/dev/null"));
        assert!(is_chain_unsafe("rg --type rust Foo >> log"));
        assert!(is_chain_unsafe("rg -n Foo < input"));
        assert!(is_chain_unsafe("rg -i Foo 2>&1 | head"));
    }

    #[test]
    fn faithful_commands_still_chain() {
        // Plain rg, grep, and the commands rtk compresses faithfully are
        // not bypassed (rtk's compression is the whole point).
        assert!(!is_chain_unsafe("rg Foo"));
        assert!(!is_chain_unsafe("rg Foo | head"));
        // Unflagged rg in a later segment also still chains.
        assert!(!is_chain_unsafe("cat f.log | rg Foo"));
        // Plain rg with a redirect is unflagged (redirect ends the scan).
        assert!(!is_chain_unsafe("rg Foo > out.txt"));
        assert!(!is_chain_unsafe(r#"rg "fn foo""#));
        assert!(!is_chain_unsafe("grep -r Foo ."));
        assert!(!is_chain_unsafe("git status"));
        assert!(!is_chain_unsafe("cat README.md"));
        // ls chains again (rtk 0.42.4 fixed its ls proxy).
        assert!(!is_chain_unsafe("ls"));
        assert!(!is_chain_unsafe("ls -la"));
        assert!(!is_chain_unsafe("cd crates && ls"));
        assert!(!is_chain_unsafe("find . -name '*.rs'"));
        assert!(!is_chain_unsafe("tree"));
        assert!(!is_chain_unsafe("head -5 README.md"));
        assert!(!is_chain_unsafe("tail -5 README.md"));
        assert!(!is_chain_unsafe(""));
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
