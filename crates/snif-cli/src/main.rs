mod commands;

use clap::{Parser, Subcommand};
use snif_config::constants::cli;
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
        #[arg(long, default_value = cli::DEFAULT_PATH)]
        path: String,

        /// Pre-warm all summaries and embeddings after building the structural
        /// graph. By default, only the structural graph is built and summaries
        /// are generated on-demand during review. Use this flag to pre-build
        /// the full semantic index upfront.
        #[arg(long)]
        full_index: bool,

        /// Reset the database and rebuild from scratch. This drops all existing
        /// indexes, summaries, and embeddings. Use when the index is corrupted
        /// or when you want a completely clean state.
        #[arg(long)]
        rebuild: bool,
    },

    /// Review a code change
    Review {
        /// Path to the repository root
        #[arg(long, default_value = cli::DEFAULT_PATH)]
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
        #[arg(long, default_value = cli::DEFAULT_OUTPUT_FORMAT)]
        format: String,
    },

    /// Remove all local runtime data (index, cache, feedback)
    Clean {
        /// Path to the repository root
        #[arg(long, default_value = cli::DEFAULT_PATH)]
        path: String,
    },

    /// Download and cache the local embedding model
    WarmEmbeddings {
        /// Path to the repository root
        #[arg(long, default_value = cli::DEFAULT_PATH)]
        path: String,
    },

    /// Run the evaluation harness against benchmark fixtures
    Eval {
        /// Path to fixtures directory
        #[arg(long)]
        fixtures: String,

        /// Path to the repository root (for config loading)
        #[arg(long, default_value = cli::DEFAULT_PATH)]
        path: String,

        /// Path to JSONL history file for tracking results over time
        #[arg(long, default_value = cli::DEFAULT_EVAL_HISTORY)]
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
        Commands::Index {
            path,
            full_index,
            rebuild,
        } => {
            commands::index::run(&path, rebuild, full_index)?;
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
        Commands::WarmEmbeddings { path } => {
            commands::warm_embeddings::run(&path)?;
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
