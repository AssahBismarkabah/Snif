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

        /// Platform: github or gitlab (auto-detected from CI env)
        #[arg(long)]
        platform: Option<String>,

        /// GitHub repository (owner/repo)
        #[arg(long)]
        repo: Option<String>,

        /// Pull request number (GitHub) or merge request IID (GitLab)
        #[arg(long)]
        pr: Option<u64>,

        /// GitLab project path (group/project)
        #[arg(long)]
        project: Option<String>,

        /// GitLab merge request IID (alias for --pr)
        #[arg(long)]
        mr: Option<u64>,

        /// Path to a local diff file (development convenience)
        #[arg(long)]
        diff_file: Option<String>,

        /// Output format: json (default) or sarif
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Remove all local runtime data (index, cache, feedback)
    Clean {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Run the evaluation harness against benchmark fixtures
    Eval {
        /// Path to fixtures directory
        #[arg(long)]
        fixtures: String,

        /// Path to the repository root (for config loading)
        #[arg(long, default_value = ".")]
        path: String,

        /// Path to JSONL history file for tracking results over time
        #[arg(long, default_value = "eval-history.jsonl")]
        history: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index { path, full } => {
            commands::index::run(&path, full)?;
        }
        Commands::Review {
            path,
            platform,
            repo,
            pr,
            project,
            mr,
            diff_file,
            format,
        } => {
            commands::review::run(
                &path,
                platform.as_deref(),
                repo.as_deref(),
                pr.or(mr),
                project.as_deref(),
                diff_file.as_deref(),
                &format,
            )?;
        }
        Commands::Clean { path } => {
            commands::clean::run(&path)?;
        }
        Commands::Eval {
            fixtures,
            path,
            history,
        } => {
            commands::eval::run(&path, &fixtures, &history)?;
        }
    }

    Ok(())
}
