use crate::commands::new::branchyard_env;
use crate::config::WtreeConfig;
use crate::docker;
use crate::ports::{interpolate, PortAssignment};
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

    // Read the original branch name for env injection
    let branch = std::fs::read_to_string(slug_dir.join(".branch"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| slug.to_string());

    // Start each repo's dev server if a serve command is defined
    for (i, repo) in config.repos.iter().enumerate() {
        let Some(serve_cmd) = &repo.commands.serve else {
            continue;
        };

        let worktree_path = slug_dir.join(&repo.name);
        let mut repo_vars = vars.clone();
        let port = ports.repo_ports.get(i).copied().unwrap_or(0);
        repo_vars.insert("port".to_string(), port.to_string());

        let cmd = interpolate(serve_cmd, &repo_vars);
        println!(
            "{} {} — {}",
            "→".cyan().bold(),
            repo.name.bold(),
            cmd.dimmed()
        );

        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(&worktree_path)
            .envs(branchyard_env(
                slug,
                &branch,
                &repo.name,
                &worktree_path,
                port,
            ))
            .spawn()
            .ok(); // detached — each process runs independently
    }

    println!("\n{} Serving \"{}\".", "✔".green().bold(), slug.bold());
    Ok(())
}
