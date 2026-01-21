mod build;
mod cli;
mod config;
mod container;
mod output;
mod paths;
mod workspace;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lfz")]
#[command(about = "Local First ZMK - Build ZMK firmware locally with ease")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Build ZMK firmware (default if no subcommand given)
    Build {
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
    },

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Build {
            board,
            shield,
            output,
            jobs,
            quiet,
            verbose,
        }) => cli::build::run(board, shield, output, jobs, quiet, verbose),

        Some(Commands::Update) => cli::update::run(),

        Some(Commands::Clean { all }) => cli::clean::run(all),

        Some(Commands::Purge) => cli::purge::run(),

        Some(Commands::Size) => cli::size::run(),

        // Default to build if no subcommand
        None => cli::build::run(None, None, "zmk-target".to_string(), None, false, false),
    }
}
