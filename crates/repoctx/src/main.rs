use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

mod gain;
mod index_cmd;
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

    match cli.cmd {
        Cmd::Index { force } => index_cmd::run(&repo_root, force, render),
        Cmd::Status { fast } => status_cmd::run(&repo_root, fast, render),
        Cmd::Symbols {
            query,
            kind,
            lang,
            limit,
        } => symbols_cmd::run(&repo_root, query, kind, lang, limit, render, gain_opts),
    }
}
