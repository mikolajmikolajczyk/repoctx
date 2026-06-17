use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

mod advisory;
mod analysis_cmd;
mod callgraph_cmd;
mod changed_cmd;
mod communities_cmd;
mod config;
mod config_cmd;
mod context_cmd;
mod definition_cmd;
mod deps_cmd;
mod export_cmd;
mod gain;
mod gain_cmd;
mod index_cmd;
mod init_cmd;
mod languages_cmd;
mod modulegraph_cmd;
mod outline_cmd;
mod output;
mod output_calls;
mod output_symbols;
mod overview_cmd;
mod prime_cmd;
mod read_cmd;
mod repo_root;
mod report_cmd;
mod resolver;
mod session_hook;
mod search_cmd;
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
    /// Textually-complete search: symbol defs + every ripgrep match (compressed).
    Search {
        pattern: String,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Expand the collapsed callee categories (external names + ambiguous
        /// candidate locations) instead of showing only counts.
        #[arg(long)]
        all_callees: bool,
    },
    /// Find direct callers of a symbol (who calls it). Static, name-based.
    Callers {
        name: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Drop ambiguous / external edges; keep only edges that resolve to a
        /// single in-repo symbol.
        #[arg(long)]
        resolved_only: bool,
    },
    /// Find direct callees of a symbol (what it calls). Static, name-based.
    Callees {
        name: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Drop ambiguous / external edges; keep only resolved ones.
        #[arg(long)]
        resolved_only: bool,
    },
    /// Transitive call graph from a symbol (static, name-based).
    Callgraph {
        name: String,
        /// Traversal depth (1 = direct edges only).
        #[arg(long, default_value_t = 3)]
        depth: u32,
        /// Direction to walk: `up` (callers), `down` (callees), or `both`.
        #[arg(long, default_value = "down")]
        direction: String,
        /// Follow only edges that resolve to a single in-repo symbol.
        #[arg(long)]
        resolved_only: bool,
    },
    /// List the modules a file imports (the import / dependency graph).
    Deps {
        /// File to inspect (repo-relative or absolute).
        file: PathBuf,
    },
    /// Find files that import a module (reverse dependencies). Matches any
    /// import specifier containing the argument as a substring.
    Rdeps {
        /// Module specifier or substring, e.g. `@adapters/storage-idb`.
        module: String,
    },
    /// Dead-code candidates: function/method symbols with zero incoming call
    /// edges (and not an entry point). Name-based; verify before deleting.
    Deadcode {
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Blast radius: everything that transitively calls `<name>` ("if I
    /// change this, what breaks"). Frames `callgraph --direction up`.
    Impact {
        name: String,
        #[arg(long, default_value_t = 3)]
        depth: u32,
        /// Count only edges that resolve to a single in-repo symbol.
        #[arg(long)]
        resolved_only: bool,
    },
    /// Detect cycles (recursion / mutual recursion) in the call graph.
    Cycles {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Detect circular imports (import graph, relative-resolved file→file).
    ImportCycles {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Module dependency map: resolved import topology + dependency-first order.
    Modules,
    /// Repo architecture in one call: totals, languages, modules, entry
    /// points, hotspots. The "dropped into an unfamiliar repo" command.
    Overview,
    /// Cluster the call graph into subsystems (Louvain) + god nodes.
    Communities,
    /// Session-start orientation digest (compact repo map) for context
    /// injection via a SessionStart hook — primes the agent to use repoctx.
    Prime,
    /// Deterministic architecture report (markdown) from graph topology: god
    /// nodes, subsystems, cross-cluster bridges, entry points, questions.
    Report {
        /// Write the markdown report to this file (e.g. REPORT.md) instead of
        /// stdout. Always writes markdown regardless of --json/--toon.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Export an interactive, self-contained HTML graph of the call graph:
    /// nodes colored by community + sized by degree, edges styled by ambiguity.
    Export {
        /// Write the HTML to this file (e.g. graph.html) instead of stdout.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Change-aware blast radius: symbols changed since a git ref + their
    /// transitive callers ("what this change touches + what it can break").
    Changed {
        /// Git ref to diff against (working tree vs REF). Default: HEAD.
        #[arg(long, default_value = "HEAD")]
        since: String,
    },
    /// Check an import boundary: list files matching `--from` that import a
    /// specifier matching `--to`. Answers "does layer A import layer B?".
    Boundary {
        /// Importer path substring (the layer doing the importing), e.g. `src/ui`.
        #[arg(long)]
        from: String,
        /// Imported specifier substring (the forbidden target), e.g. `@adapters`.
        #[arg(long)]
        to: String,
        /// CI gate: exit 1 if any crossing exists.
        #[arg(long)]
        forbid: bool,
    },
    /// Print the per-language coverage matrix (how well repoctx
    /// indexes each language). Agents check this before deciding to
    /// fall back to ripgrep.
    Languages,
    /// Read or write the per-repo settings table.
    Config {
        #[command(subcommand)]
        sub: ConfigSub,
    },
    /// Onboard an agent: install guidance files + (Claude) the SessionStart
    /// hook that primes context with `repoctx prime`.
    Init {
        /// Install at user-global scope (`~/.claude/`) instead of this repo.
        #[arg(short = 'g', long)]
        global: bool,
        /// Agent to set up (`claude`, `codex`, `opencode`, …).
        #[arg(long, default_value = "claude")]
        agent: String,
        /// Remove the repoctx SessionStart hook (inverse of install).
        #[arg(long)]
        uninstall: bool,
        /// Skip interactive prompts; take defaults / flags.
        #[arg(short = 'y', long)]
        yes: bool,
        /// Overwrite destinations whose content differs.
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
        Cmd::Search {
            pattern,
            lang,
            limit,
            all_callees,
        } => search_cmd::run(
            &repo_root,
            pattern,
            lang,
            limit,
            all_callees,
            render,
            gain_opts,
        ),
        Cmd::Callers {
            name,
            limit,
            resolved_only,
        } => callgraph_cmd::run(
            &repo_root,
            name,
            callgraph_cmd::Edges::Callers,
            limit,
            resolved_only,
            render,
            gain_opts,
        ),
        Cmd::Callees {
            name,
            limit,
            resolved_only,
        } => callgraph_cmd::run(
            &repo_root,
            name,
            callgraph_cmd::Edges::Callees,
            limit,
            resolved_only,
            render,
            gain_opts,
        ),
        Cmd::Callgraph {
            name,
            depth,
            direction,
            resolved_only,
        } => {
            let dir = callgraph_cmd::Direction::parse(&direction)?;
            callgraph_cmd::run_graph(
                &repo_root,
                name,
                depth,
                dir,
                resolved_only,
                render,
                gain_opts,
            )
        }
        Cmd::Deps { file } => deps_cmd::run_deps(&repo_root, file, render, gain_opts),
        Cmd::Rdeps { module } => deps_cmd::run_rdeps(&repo_root, module, render, gain_opts),
        Cmd::Boundary { from, to, forbid } => {
            deps_cmd::run_boundary(&repo_root, from, to, forbid, render, gain_opts)
        }
        Cmd::Deadcode { lang, limit } => {
            analysis_cmd::run_deadcode(&repo_root, lang, limit, render, gain_opts)
        }
        Cmd::Impact {
            name,
            depth,
            resolved_only,
        } => analysis_cmd::run_impact(&repo_root, name, depth, resolved_only, render, gain_opts),
        Cmd::Cycles { limit } => analysis_cmd::run_cycles(&repo_root, limit, render, gain_opts),
        Cmd::ImportCycles { limit } => {
            modulegraph_cmd::run_import_cycles(&repo_root, limit, render, gain_opts)
        }
        Cmd::Modules => modulegraph_cmd::run_modules(&repo_root, render, gain_opts),
        Cmd::Overview => overview_cmd::run(&repo_root, render, gain_opts),
        Cmd::Communities => communities_cmd::run(&repo_root, render, gain_opts),
        Cmd::Prime => prime_cmd::run(&repo_root),
        Cmd::Report { out } => report_cmd::run(&repo_root, render, gain_opts, out),
        Cmd::Export { out } => export_cmd::run(&repo_root, out),
        Cmd::Changed { since } => changed_cmd::run(&repo_root, since, render, gain_opts),
        Cmd::Init {
            global,
            agent,
            uninstall,
            yes,
            force,
            dry_run,
        } => {
            if uninstall {
                init_cmd::run_uninstall(&repo_root, global, dry_run)
            } else {
                let opts = init_cmd::InitOpts {
                    global,
                    agent,
                    yes,
                    force,
                    dry_run,
                };
                init_cmd::run(&repo_root, opts)
            }
        }
        Cmd::Gain {
            since,
            all,
            history,
            sub,
        } => {
            let window = gain_cmd::resolve_window(since.as_deref(), all)?;
            match sub {
                Some(GainSub::Top {
                    by,
                    since: sub_since,
                    all: sub_all,
                }) => {
                    let window = if sub_since.is_some() || sub_all {
                        gain_cmd::resolve_window(sub_since.as_deref(), sub_all)?
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
