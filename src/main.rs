mod build;
mod cli;
mod config;
mod container;
mod output;
mod paths;
mod workspace;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

/// Build options shared between top-level and `build` subcommand
#[derive(Args, Clone)]
struct BuildArgs {
    /// Build specific board (skips build.yaml)
    #[arg(short, long)]
    board: Option<String>,

    /// Build specific shield
    #[arg(short, long)]
    shield: Option<String>,

    /// Output directory for firmware files
    #[arg(short, long, default_value = "zmk-target")]
    output: String,

    /// Number of parallel builds (default: number of targets)
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Suppress build output
    #[arg(long)]
    quiet: bool,

    /// Stream real-time build output for each target
    #[arg(short, long)]
    verbose: bool,

    /// Incremental build (faster, but may have stale artifacts)
    #[arg(short, long)]
    incremental: bool,

    /// Build only targets in this group (e.g., "central", "peripheral", or "all")
    #[arg(short, long, default_value = "all")]
    group: String,
}

#[derive(Parser)]
#[command(name = "lfz")]
#[command(about = "Local First ZMK - Build ZMK firmware locally with ease")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Top-level build options (used when no subcommand is given)
    #[command(flatten)]
    build_args: BuildArgs,
}

#[derive(Subcommand)]
enum Commands {
    /// Build ZMK firmware (default if no subcommand given)
    Build(BuildArgs),

    /// Refresh west workspace (re-run west update)
    Update,

    /// Remove cached workspace for this config
    Clean {
        /// Remove all cached workspaces
        #[arg(long)]
        all: bool,
    },

    /// Remove all caches (workspaces + ccache)
    Purge,

    /// Show disk space used by caches
    Size,
}

fn run_build(args: BuildArgs) -> Result<()> {
    cli::build::run(
        args.board,
        args.shield,
        args.output,
        args.jobs,
        args.quiet,
        args.verbose,
        args.incremental,
        args.group,
    )
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Build(args)) => run_build(args),
        Some(Commands::Update) => cli::update::run(),
        Some(Commands::Clean { all }) => cli::clean::run(all),
        Some(Commands::Purge) => cli::purge::run(),
        Some(Commands::Size) => cli::size::run(),
        // Default to build with top-level args
        None => run_build(cli.build_args),
    }
}
