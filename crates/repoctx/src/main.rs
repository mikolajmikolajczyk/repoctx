use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

mod definition_cmd;
mod gain;
mod gain_cmd;
mod index_cmd;
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

    /// Don't auto-run `index` when a read command finds no index;
    /// bail with `no index found` instead. Useful for scripts that
    /// want to assert the index is already present.
    #[arg(long, global = true)]
    no_auto_index: bool,

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let render = output::resolve(cli.json, cli.toon);
    let repo_root = repo_root::resolve(cli.repo)?;
    let gain_opts = gain::GainOpts::from_cli(cli.no_record, cli.record_query);

    let no_auto_index = cli.no_auto_index;

    match cli.cmd {
        Cmd::Index { force } => index_cmd::run(&repo_root, force, render),
        Cmd::Outline { file } => {
            outline_cmd::run(&repo_root, file, render, gain_opts, no_auto_index)
        }
        Cmd::Definition { name, lang, limit } => definition_cmd::run(
            &repo_root,
            name,
            lang,
            limit,
            render,
            gain_opts,
            no_auto_index,
        ),
        Cmd::Status { fast } => status_cmd::run(&repo_root, fast, render, no_auto_index),
        Cmd::Symbols {
            query,
            kind,
            lang,
            limit,
        } => symbols_cmd::run(
            &repo_root,
            query,
            kind,
            lang,
            limit,
            render,
            gain_opts,
            no_auto_index,
        ),
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
                    gain_cmd::run_top(&repo_root, window, by, render, no_auto_index)
                }
                None => gain_cmd::run_summary(&repo_root, window, render, history, no_auto_index),
            }
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
