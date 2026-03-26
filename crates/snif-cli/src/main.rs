mod commands;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "snif")]
#[command(version)]
#[command(about = "Repository-aware code review agent")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the repository index
    Index {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        path: String,

        /// Force a full rebuild instead of incremental update
        #[arg(long)]
        full: bool,
    },

    /// Review a code change
    Review {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        path: String,

        /// GitHub repository (owner/repo)
        #[arg(long)]
        repo: Option<String>,

        /// Pull request number
        #[arg(long)]
        pr: Option<u64>,

        /// Path to a local diff file (development convenience)
        #[arg(long)]
        diff_file: Option<String>,
    },

    /// Run the evaluation harness
    Eval,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index { path, full } => {
            commands::index::run(&path, full)?;
        }
        Commands::Review {
            path,
            repo,
            pr,
            diff_file,
        } => {
            commands::review::run(&path, repo.as_deref(), pr, diff_file.as_deref())?;
        }
        Commands::Eval => {
            commands::eval::run()?;
        }
    }

    Ok(())
}
