use crate::config::WtreeConfig;
use crate::docker;
use crate::ports::PortAssignment;
use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(slug: &str) -> Result<()> {
    let (config, workspace_root) = WtreeConfig::load()?;
    let worktrees_base = config.worktrees_path(&workspace_root);
    let slug_dir = worktrees_base.join(slug);

    if !slug_dir.exists() {
        anyhow::bail!(
            "Worktree \"{}\" not found.\n  Use `branchyard list` to see active worktrees.\n  Use `branchyard new {}` to create it.",
            slug,
            slug
        );
    }

    // Read the assigned slot
    let slot = std::fs::read_to_string(slug_dir.join(".slot"))
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(0);

    let ports = PortAssignment::new(
        config.ports.base,
        slot,
        config.repos.len(),
        config.services.len(),
    );
    let mut vars = ports.variable_map(slug, &config.repos, &config.services);
    vars.insert(
        "workspace".to_string(),
        workspace_root.to_string_lossy().to_string(),
    );

    // Start Docker services if any are configured
    if !config.services.is_empty() {
        println!("{}", "Starting Docker services...".dimmed());
        docker::up(slug, &slug_dir)?;
        println!("  {} Docker services running.", "✔".green().bold());
    }

    println!("\n{} Serving \"{}\".", "✔".green().bold(), slug.bold());
    Ok(())
}
