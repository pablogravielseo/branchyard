use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod commands;
mod config;
mod docker;
mod ports;
mod terminal;
mod worktree;

#[derive(Parser)]
#[command(
    name = "branchyard",
    version,
    about = "Paired Git Worktrees CLI",
    long_about = "Create and manage paired Git worktrees across multiple repos with isolated runtime environments."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure branchyard for this workspace (creates .wtree.yml)
    Init,

    /// Create paired worktrees for a feature
    New {
        /// Branch name to create (e.g. pgs-rd-item-20-payment, feature/auth-jwt)
        branch: String,

        /// Directory name for the worktree (defaults to the branch name if not set)
        #[arg(long, short)]
        slug: Option<String>,
    },

    /// Start services for a worktree
    Serve {
        /// Feature slug
        slug: String,
    },

    /// Stop services for a worktree
    Stop {
        /// Feature slug
        slug: String,
    },

    /// List active worktrees
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove worktree and clean up environment
    Done {
        /// Feature slug(s) — pass multiple to clean up several at once
        #[arg(required_unless_present = "all")]
        slugs: Vec<String>,

        /// Remove all active worktrees
        #[arg(long, short)]
        all: bool,
    },

    /// Generate shell completion script
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::run(),
        Commands::New { branch, slug } => {
            let slug = slug.unwrap_or_else(|| branch.clone());
            commands::new::run(&branch, &slug)
        }
        Commands::Serve { slug } => commands::serve::run(&slug),
        Commands::Stop { slug } => commands::stop::run(&slug),
        Commands::List { json } => commands::list::run(json),
        Commands::Done { slugs, all } => commands::done::run_multi(slugs, all),
        Commands::Completion { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "branchyard", &mut std::io::stdout());
            Ok(())
        }
    }
}
