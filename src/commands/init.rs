use anyhow::Result;
use inquire::{Confirm, MultiSelect, Select, Text};
use owo_colors::OwoColorize;
use std::path::Path;

use crate::config::{
    HooksConfig, Multiplexer, PortConfig, RepoCommands, RepoConfig, ServiceConfig, SetupConfig,
    TerminalConfig, WtreeConfig, CONFIG_FILE,
};

const COMMON_SERVICES: &[(&str, &str, u16)] = &[
    ("postgres", "postgres:16", 5432),
    ("mysql", "mysql:8", 3306),
    ("redis", "redis:7", 6379),
    ("mongodb", "mongo:7", 27017),
    ("kafka", "confluentinc/cp-kafka:7", 9092),
    ("elasticsearch", "elasticsearch:8", 9200),
    ("minio", "minio/minio:latest", 9000),
];

pub fn run() -> Result<()> {
    if Path::new(CONFIG_FILE).exists() {
        let overwrite = Confirm::new(&format!("{CONFIG_FILE} already exists. Overwrite?"))
            .with_default(false)
            .prompt()?;

        if !overwrite {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!(
        "{}\n",
        "Configuring branchyard for this workspace...".bold()
    );

    let base_branch = Text::new("Base branch:").with_default("main").prompt()?;

    let worktrees_dir = Text::new("Worktrees directory:")
        .with_default("./worktrees")
        .prompt()?;

    // ── Repos ────────────────────────────────────────────────────────────────

    let mut repos: Vec<RepoConfig> = Vec::new();

    loop {
        let n = repos.len() + 1;
        println!("\n{}", format!("Repository {n}").bold());

        let name = Text::new("  Name (e.g. frontend, backend, api):").prompt()?;
        let path = Text::new(&format!("  Relative path (e.g. ./{name}):")).prompt()?;

        let serve = Text::new("  Serve command (leave blank to skip):")
            .with_help_message("Use {port} for the assigned port, {slug} for the worktree name")
            .prompt_skippable()?
            .filter(|s| !s.is_empty());

        let setup = Text::new("  One-time setup command (leave blank to skip):")
            .with_help_message("Runs once on `branchyard new`. Use {port}, {slug}, {db_port}, etc.")
            .prompt_skippable()?
            .filter(|s| !s.is_empty());

        repos.push(RepoConfig {
            name,
            path,
            commands: RepoCommands {
                serve,
                stop: None,
                setup,
            },
            setup: SetupConfig { symlinks: vec![] },
        });

        let more = Confirm::new("Add another repository?")
            .with_default(false)
            .prompt()?;

        if !more {
            break;
        }
    }

    // ── Services ─────────────────────────────────────────────────────────────

    println!("\n{}", "Services (Docker)".bold());

    let service_labels: Vec<&str> = COMMON_SERVICES.iter().map(|(name, _, _)| *name).collect();
    let selected = MultiSelect::new("Select services to include:", service_labels)
        .with_help_message("Space to select, Enter to confirm. Leave empty for none.")
        .prompt()?;

    let services: Vec<ServiceConfig> = selected
        .iter()
        .filter_map(|name| {
            COMMON_SERVICES
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(name, image, port)| ServiceConfig {
                    name: name.to_string(),
                    image: image.to_string(),
                    port: *port,
                    environment: vec![],
                    command: None,
                    depends_on: vec![],
                    volumes: vec![],
                    env_file: None,
                    platform: None,
                    build: None,
                })
        })
        .collect();

    // ── Ports ─────────────────────────────────────────────────────────────────

    println!("\n{}", "Ports".bold());
    let port_base = Text::new("Base port:")
        .with_default("3000")
        .with_help_message("Each worktree slot gets ports starting here, offset by 10 per slot")
        .prompt()?
        .parse::<u16>()
        .unwrap_or(3000);

    // ── Terminal ──────────────────────────────────────────────────────────────

    println!("\n{}", "Terminal".bold());
    let multiplexer_choice = Select::new(
        "Terminal integration:",
        vec!["none", "warp", "iterm2", "tmux", "ghostty"],
    )
    .with_help_message("branchyard will open a pre-configured session on `branchyard new`")
    .prompt()?;

    let multiplexer = match multiplexer_choice {
        "warp" => Multiplexer::Warp,
        "iterm2" => Multiplexer::Iterm2,
        "tmux" => Multiplexer::Tmux,
        "ghostty" => Multiplexer::Ghostty,
        _ => Multiplexer::None,
    };

    // ── Write config ──────────────────────────────────────────────────────────

    let config = WtreeConfig {
        base_branch,
        worktrees_dir,
        repos,
        ports: PortConfig { base: port_base },
        services,
        terminal: TerminalConfig {
            multiplexer,
            autostart: false,
        },
        hooks: HooksConfig::default(),
    };

    config.save(Path::new(CONFIG_FILE))?;

    println!("\n{} {CONFIG_FILE} created.", "✔".green().bold());
    println!(
        "  Run {} to create your first worktree.",
        "`branchyard new <slug>`".bold()
    );
    println!(
        "  Edit {} to add symlinks, hooks, or custom service config.",
        CONFIG_FILE.dimmed()
    );

    Ok(())
}
