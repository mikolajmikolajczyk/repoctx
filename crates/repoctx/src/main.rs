use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

mod advisory;
mod config;
mod config_cmd;
mod context_cmd;
mod definition_cmd;
mod gain;
mod gain_cmd;
mod hook_cmd;
mod hook_marker;
mod hook_rewrite;
mod hook_scan;
mod hook_script;
mod hook_takeover;
mod index_cmd;
mod init_cmd;
mod languages_cmd;
mod outline_cmd;
mod output;
mod output_symbols;
#[cfg(test)]
mod output_tests;
mod read_cmd;
mod repo_root;
mod status_cmd;
mod symbols_cmd;
mod walk;

#[derive(Parser, Debug)]
#[command(
    name = "repoctx",
    version,
    about = "AI-oriented repository intelligence CLI"
)]
struct Cli {
    /// Force JSON machine output.
    #[arg(long, global = true, conflicts_with = "toon")]
    json: bool,

    /// Force TOON machine output (even on a TTY).
    #[arg(long, global = true)]
    toon: bool,

    /// Repository search start (default: current directory).
    #[arg(long, global = true, value_name = "PATH")]
    repo: Option<PathBuf>,

    /// Verbosity: -v = info, -vv = debug. `RUST_LOG` overrides.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Skip gain analytics recording for this invocation.
    #[arg(long, global = true)]
    no_record: bool,

    /// Persist the query string in the usage row (off by default).
    #[arg(long, global = true)]
    record_query: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Build or update the on-disk index for the repository.
    Index {
        /// Reparse every file even if (mtime, size) is unchanged.
        #[arg(long)]
        force: bool,
    },
    /// Report index health, counts, and staleness.
    Status {
        /// Skip the staleness stat-walk.
        #[arg(long)]
        fast: bool,
    },
    /// Print the symbol tree of one file.
    Outline {
        /// File path (repo-relative or absolute).
        file: PathBuf,
    },
    /// Find exact-name definitions.
    Definition {
        name: String,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Print a symbol plus surrounding source lines.
    Context {
        symbol: String,
        /// Lines of leading and trailing context. Default 5.
        #[arg(long, default_value_t = 5)]
        context: usize,
        /// Maximum number of matches. Default 3.
        #[arg(long, default_value_t = 3)]
        limit: usize,
    },
    /// Search indexed symbols by case-insensitive substring.
    Symbols {
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Print the per-language coverage matrix (how well repoctx
    /// indexes each language). Agents check this before deciding to
    /// fall back to ripgrep.
    Languages,
    /// Decide whether a bash command would be rewritten (debug/bench).
    /// Exit 0 + the rewritten command on stdout if rewritten; exit 1 on
    /// passthrough. Mirrors the hook's decision without JSON wrapping.
    Rewrite {
        /// The bash command to test, e.g. `repoctx rewrite 'rg foo'`.
        command: String,
    },
    /// Read or write the per-repo settings table.
    Config {
        #[command(subcommand)]
        sub: ConfigSub,
    },
    /// Manage per-agent integration files (skills, AGENTS.md fragments).
    Hook {
        #[command(subcommand)]
        sub: HookSub,
    },
    /// Install the repoctx hook + agent guidance (first-class onboarding).
    Init {
        /// Install at user-global scope (`~/.claude/`) instead of this repo.
        #[arg(short = 'g', long)]
        global: bool,
        /// Agent to set up. Only `claude` is supported today.
        #[arg(long, default_value = "claude")]
        agent: String,
        /// Chain rtk underneath: `auto` (when on PATH) | `on` | `off`.
        #[arg(long, default_value = "auto")]
        rtk: String,
        /// Skip interactive prompts; take defaults / flags.
        #[arg(short = 'y', long)]
        yes: bool,
        /// Overwrite agent files whose content differs.
        #[arg(long)]
        force: bool,
        /// Print the plan; write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Surface navigation cost avoided by querying through repoctx.
    Gain {
        /// Lower bound: `7d`, `2h`, `30m`, `120s`. Defaults to 30 days.
        #[arg(long, conflicts_with = "all")]
        since: Option<String>,

        /// Drop the window entirely (all-time totals).
        #[arg(long)]
        all: bool,

        /// Show the N most recent invocations in the window (default: 20 with no value).
        #[arg(long, num_args = 0..=1, default_missing_value = "20")]
        history: Option<usize>,

        #[command(subcommand)]
        sub: Option<GainSub>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigSub {
    /// Print every effective config key with its current value + source.
    Show,
    /// Print one key's current value.
    Get { key: String },
    /// Validate + write one key.
    Set { key: String, value: String },
    /// Delete one key (default applies again).
    Unset { key: String },
}

#[derive(Subcommand, Debug)]
enum HookSub {
    /// List available agents and their descriptions.
    List,
    /// Show which destination files exist for each agent in the target dir.
    Status {
        /// Target directory. Defaults to the repo root.
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
    },
    /// PreToolUse hook handler — Claude Code calls this with the
    /// tool-use JSON on stdin. Rewrites recognized `rg`/`grep`
    /// patterns to `repoctx` commands; on passthrough, chains
    /// `rtk hook claude` when `--rtk-chain=1` (or `hook.use_rtk`).
    Claude {
        /// Chain `rtk hook claude` on passthrough: `0` (off) or `1` (on).
        /// Omitted → resolve from `hook.use_rtk` config.
        #[arg(long, value_name = "0|1")]
        rtk_chain: Option<u8>,
        /// Probe used by the generated hook script's version guard:
        /// exit 0 if this binary understands `--rtk-chain`.
        #[arg(long, hide = true)]
        supports_rtk_chain: bool,
    },
    /// Re-take ownership of `.claude/settings.json` PreToolUse → Bash
    /// matcher if another installer (rtk reinstall, manual edit, …)
    /// added a sibling entry. Idempotent; safe to run anytime.
    Doctor {
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
        /// Plan the doctor pass; do not write.
        #[arg(long)]
        dry_run: bool,
    },
    /// Install one agent's files into the target dir.
    Install {
        /// Agent name (`claude`, `codex`, `opencode`).
        agent: String,
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
        /// Plan the install without touching the filesystem.
        #[arg(long)]
        dry_run: bool,
        /// Overwrite write-mode destinations even when current content differs.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
enum GainSub {
    /// Per-command breakdown ranked by savings (default) or reduction ratio.
    Top {
        /// `saved` (default) or `ratio`.
        #[arg(long, default_value = "saved")]
        by: String,

        #[arg(long, conflicts_with = "all")]
        since: Option<String>,

        #[arg(long)]
        all: bool,
    },
}

fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        _ => Level::DEBUG,
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default.to_string()));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

fn main() {
    if let Err(e) = run() {
        // Clean, single-line error to stderr — no anyhow Debug backtrace
        // dump (agents/CI commonly set RUST_BACKTRACE, which would
        // otherwise leak frames into the message). Opt back in with
        // REPOCTX_BACKTRACE=1 for the full chain + captured backtrace.
        if std::env::var_os("REPOCTX_BACKTRACE").is_some() {
            eprintln!("error: {e:?}");
        } else {
            eprintln!("error: {e:#}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let repo_root = repo_root::resolve(cli.repo)?;
    let cfg = load_config(&repo_root);
    let render = output::resolve(cli.json, cli.toon, cfg.output.default);
    let gain_opts = gain::GainOpts::from_cli(cli.no_record, cli.record_query, &cfg.gain);

    match cli.cmd {
        Cmd::Index { force } => index_cmd::run(&repo_root, force, render),
        Cmd::Outline { file } => outline_cmd::run(&repo_root, file, render, gain_opts),
        Cmd::Definition { name, lang, limit } => {
            definition_cmd::run(&repo_root, name, lang, limit, render, gain_opts)
        }
        Cmd::Context {
            symbol,
            context,
            limit,
        } => context_cmd::run(&repo_root, symbol, context, limit, render, gain_opts),
        Cmd::Languages => languages_cmd::run(render),
        Cmd::Rewrite { command } => {
            // Exit-code protocol (e06f463): 0 = rewrite (stdout = command),
            // 1 = passthrough. 2/3 (deny/ask) reserved for future rules.
            match hook_rewrite::try_semantic_rewrite(&command) {
                Some((rewritten, _rule)) => {
                    println!("{rewritten}");
                    std::process::exit(0);
                }
                None => std::process::exit(1),
            }
        }
        Cmd::Config { sub } => match sub {
            ConfigSub::Show => config_cmd::run_show(&repo_root, render),
            ConfigSub::Get { key } => config_cmd::run_get(&repo_root, key, render),
            ConfigSub::Set { key, value } => config_cmd::run_set(&repo_root, key, value),
            ConfigSub::Unset { key } => config_cmd::run_unset(&repo_root, key),
        },
        Cmd::Status { fast } => status_cmd::run(&repo_root, fast, render),
        Cmd::Symbols {
            query,
            kind,
            lang,
            limit,
        } => symbols_cmd::run(&repo_root, query, kind, lang, limit, render, gain_opts),
        Cmd::Hook { sub } => match sub {
            HookSub::Claude {
                rtk_chain,
                supports_rtk_chain,
            } => {
                if supports_rtk_chain {
                    std::process::exit(0); // version-guard probe
                }
                let code = hook_rewrite::run(&cfg.hook, rtk_chain.map(|v| v != 0))?;
                std::process::exit(code);
            }
            HookSub::Doctor { dir, dry_run } => {
                let target = hook_cmd::resolve_dir(dir, &repo_root);
                hook_cmd::run_doctor(&repo_root, &target, dry_run, render)
            }
            HookSub::List => hook_cmd::run_list(render),
            HookSub::Status { dir } => {
                let target = hook_cmd::resolve_dir(dir, &repo_root);
                hook_cmd::run_status(&target, render)
            }
            HookSub::Install {
                agent,
                dir,
                dry_run,
                force,
            } => {
                let target = hook_cmd::resolve_dir(dir, &repo_root);
                let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("repoctx"));
                hook_cmd::run_install(&target, &repo_root, &agent, dry_run, force, &bin, render)
            }
        },
        Cmd::Init {
            global,
            agent,
            rtk,
            yes,
            force,
            dry_run,
        } => {
            let opts = init_cmd::InitOpts {
                global,
                agent,
                rtk: config::HookUseRtk::parse(&rtk)?,
                yes,
                force,
                dry_run,
            };
            init_cmd::run(&repo_root, opts)
        }
        Cmd::Gain {
            since,
            all,
            history,
            sub,
        } => {
            let window = resolve_window(since.as_deref(), all)?;
            match sub {
                Some(GainSub::Top {
                    by,
                    since: sub_since,
                    all: sub_all,
                }) => {
                    let window = if sub_since.is_some() || sub_all {
                        resolve_window(sub_since.as_deref(), sub_all)?
                    } else {
                        window
                    };
                    let by = gain_cmd::TopBy::parse(&by)?;
                    gain_cmd::run_top(&repo_root, window, by, render)
                }
                None => gain_cmd::run_summary(&repo_root, window, render, history),
            }
        }
    }
}

/// Best-effort load. Failure (missing DB, IO error) falls back to
/// built-in defaults — we don't want a stale or broken settings table
/// to break read commands. Real config errors get logged once.
fn load_config(repo_root: &std::path::Path) -> config::Config {
    if !repo_root.join(".repoctx/index.db").exists() {
        return config::Config::defaults();
    }
    match repoctx_store::Store::open(repo_root) {
        Ok(store) => config::Config::load(&store).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "config: load failed; using defaults");
            config::Config::defaults()
        }),
        Err(e) => {
            tracing::warn!(error = %e, "config: store open failed; using defaults");
            config::Config::defaults()
        }
    }
}

fn resolve_window(since: Option<&str>, all: bool) -> Result<gain_cmd::Window> {
    if all {
        return Ok(gain_cmd::Window::All);
    }
    match since {
        Some(s) => Ok(gain_cmd::Window::Since(gain_cmd::parse_since(s)?)),
        None => Ok(gain_cmd::default_window()),
    }
}
