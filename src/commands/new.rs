use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use std::os::unix::fs::symlink;
use std::time::Duration;

use crate::config::WtreeConfig;
use crate::docker;
use crate::ports::{interpolate, next_available_slot, read_used_slots, PortAssignment};
use crate::terminal;
use crate::worktree;

pub fn run(branch: &str, slug: &str) -> Result<()> {
    let (config, workspace_root) = WtreeConfig::load()?;
    let worktrees_base = config.worktrees_path(&workspace_root);
    let slug_dir = worktrees_base.join(slug);

    if slug_dir.exists() {
        anyhow::bail!(
            "Slug \"{}\" already exists.\n  Use `branchyard list` to see active worktrees.\n  Use `branchyard done {}` to remove the existing one.",
            slug,
            slug
        );
    }

    std::fs::create_dir_all(&slug_dir)?;

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("valid spinner template"),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));

    // Assign a port slot
    let used = read_used_slots(&worktrees_base);
    let slot = next_available_slot(&used);
    let ports = PortAssignment::new(
        config.ports.base,
        slot,
        config.repos.len(),
        config.services.len(),
    );

    // Persist the slot and branch name for later use (serve, done, list)
    std::fs::write(slug_dir.join(".slot"), slot.to_string())?;
    std::fs::write(slug_dir.join(".branch"), branch)?;

    // Build shared variable map (all repos + all services)
    let mut vars = ports.variable_map(slug, &config.repos, &config.services);
    vars.insert(
        "workspace".to_string(),
        workspace_root.to_string_lossy().to_string(),
    );

    // ── Worktrees ────────────────────────────────────────────────────────────

    for (i, repo) in config.repos.iter().enumerate() {
        let repo_path = config.repo_path(&repo.path, &workspace_root);
        let worktree_path = slug_dir.join(&repo.name);

        spinner.set_message(format!("Creating worktree for {}...", repo.name));
        worktree::create(&repo_path, &worktree_path, branch)?;

        spinner.println(format!(
            "  {} Worktree {} created  (branch: {}  port: {})",
            "✔".green().bold(),
            repo.name.bold(),
            branch.cyan(),
            ports.repo_ports.get(i).copied().unwrap_or(0)
        ));

        // Symlinks declared in setup.symlinks
        for link in &repo.setup.symlinks {
            let from = workspace_root.join(&link.from);
            let to = worktree_path.join(&link.to);

            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if !to.exists() {
                spinner.set_message(format!("Symlinking {} → {}", link.from, link.to));
                symlink(&from, &to)?;
                spinner.println(format!(
                    "  {} Symlink: {} → {}",
                    "✔".green().bold(),
                    link.from.dimmed(),
                    link.to.dimmed()
                ));
            }
        }

        // One-time setup command
        if let Some(setup_cmd) = &repo.commands.setup {
            let mut repo_vars = vars.clone();
            if let Some(&port) = ports.repo_ports.get(i) {
                repo_vars.insert("port".to_string(), port.to_string());
            }
            let cmd = interpolate(setup_cmd, &repo_vars);

            spinner.set_message(format!("Setup ({}): {}", repo.name, cmd));
            let port = ports.repo_ports.get(i).copied().unwrap_or(0);
            let status = std::process::Command::new("sh")
                .args(["-c", &cmd])
                .current_dir(&worktree_path)
                .envs(branchyard_env(
                    slug,
                    branch,
                    &repo.name,
                    &worktree_path,
                    port,
                ))
                .status()?;

            if !status.success() {
                spinner.println(format!(
                    "  {} Setup command failed for {} — continuing",
                    "⚠".yellow().bold(),
                    repo.name
                ));
            } else {
                spinner.println(format!(
                    "  {} Setup done for {}",
                    "✔".green().bold(),
                    repo.name.bold()
                ));
            }
        }
    }

    // ── Docker services ───────────────────────────────────────────────────────

    if !config.services.is_empty() {
        spinner.set_message("Generating docker-compose.override.yml...");
        let override_content = docker::generate_override(slug, &config, &ports, &workspace_root);
        docker::write_override(&slug_dir, &override_content)?;
        spinner.println(format!(
            "  {} docker-compose.override.yml generated",
            "✔".green().bold()
        ));
    }

    // ── Terminal integration ──────────────────────────────────────────────────

    use crate::config::Multiplexer;
    if config.terminal.multiplexer != Multiplexer::None {
        spinner.set_message("Creating terminal launch config...");
        terminal::open(slug, &config, &worktrees_base, &ports, &vars)?;
        spinner.println(format!(
            "  {} Terminal launch config created",
            "✔".green().bold()
        ));
    }

    // ── after_new hooks ───────────────────────────────────────────────────────

    for hook in &config.hooks.after_new {
        let cmd = interpolate(hook, &vars);
        spinner.set_message(format!("Hook: {cmd}"));
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .status()?;
    }

    spinner.finish_and_clear();

    // ── Summary table ─────────────────────────────────────────────────────────

    println!(
        "\n{}",
        format!("Worktree \"{}\" ready", slug).green().bold()
    );
    println!();

    // Repo ports
    let name_w = config
        .repos
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(8)
        .max(7);
    println!(
        "{:<width$}  {}",
        "REPO".bold(),
        "PORT".bold(),
        width = name_w
    );
    println!("{}", "─".repeat(name_w + 8).dimmed());
    for (i, repo) in config.repos.iter().enumerate() {
        let port = ports.repo_ports.get(i).copied().unwrap_or(0);
        println!("{:<width$}  {}", repo.name, port, width = name_w);
    }

    if !config.services.is_empty() {
        println!();
        let svc_name_w = config
            .services
            .iter()
            .map(|s| s.name.len())
            .max()
            .unwrap_or(7)
            .max(7);
        println!(
            "{:<width$}  {}  {}",
            "SERVICE".bold(),
            "HOST PORT".bold(),
            "IMAGE".bold(),
            width = svc_name_w
        );
        println!("{}", "─".repeat(svc_name_w + 30).dimmed());
        for (i, svc) in config.services.iter().enumerate() {
            let port = ports.service_ports.get(i).copied().unwrap_or(svc.port);
            println!(
                "{:<width$}  {:<9}  {}",
                svc.name,
                port,
                svc.image,
                width = svc_name_w
            );
        }
    }

    println!();
    println!(
        "  Run {} to start services.",
        format!("`branchyard serve {slug}`").bold()
    );

    Ok(())
}

/// Build the set of BRANCHYARD_* environment variables injected into every
/// shell command (setup, serve, stop). Scripts can read these without
/// Branchyard knowing anything about what those scripts actually do.
pub fn branchyard_env(
    slug: &str,
    branch: &str,
    repo_name: &str,
    worktree_path: &std::path::Path,
    port: u16,
) -> Vec<(String, String)> {
    vec![
        ("BRANCHYARD_SLUG".into(), slug.to_string()),
        ("BRANCHYARD_BRANCH".into(), branch.to_string()),
        ("BRANCHYARD_REPO".into(), repo_name.to_string()),
        (
            "BRANCHYARD_WORKTREE_PATH".into(),
            worktree_path.to_string_lossy().to_string(),
        ),
        ("BRANCHYARD_PORT".into(), port.to_string()),
    ]
}
