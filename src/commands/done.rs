use crate::config::WtreeConfig;
use crate::docker;
use crate::ports::{interpolate, PortAssignment};
use crate::terminal;
use crate::worktree;
use anyhow::Result;
use inquire::{Confirm, Select};
use owo_colors::OwoColorize;
use std::path::Path;

/// Kill all processes whose working directory is inside the worktree directory.
/// Uses `lsof` to find processes by cwd — works on macOS and Linux.
/// Also stops Watchman watches on the directory to avoid getcwd errors.
fn kill_processes_in_dir(slug_dir: &Path) {
    let dir_str = match slug_dir.to_str() {
        Some(s) => s,
        None => return,
    };

    // Stop Watchman from watching this directory before removing it
    let _ = std::process::Command::new("watchman")
        .args(["watch-del", dir_str])
        .output();

    // lsof -t -a +d <dir> lists PIDs of processes with cwd inside dir
    let output = std::process::Command::new("lsof")
        .args(["-t", "-a", "+d", dir_str])
        .output();

    let Ok(output) = output else { return };

    let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        return;
    }

    for pid in &pids {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }

    // Give processes a moment to exit gracefully before files are removed
    std::thread::sleep(std::time::Duration::from_millis(500));
}

pub fn run_multi(slugs: Vec<String>, all: bool) -> Result<()> {
    let targets = if all {
        let (config, workspace_root) = WtreeConfig::load()?;
        let worktrees_base = config.worktrees_path(&workspace_root);
        let mut found = Vec::new();
        if worktrees_base.exists() {
            for entry in std::fs::read_dir(&worktrees_base)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        found.push(name.to_string());
                    }
                }
            }
        }
        if found.is_empty() {
            println!("No active worktrees found.");
            return Ok(());
        }
        found.sort();
        found
    } else {
        slugs
    };

    for slug in &targets {
        println!("{}", format!("── done: {slug} ──").bold());
        if let Err(e) = run(slug) {
            eprintln!("  {} {}: {}", "✗".red().bold(), slug, e);
        }
    }
    Ok(())
}

pub fn run(slug: &str) -> Result<()> {
    let (config, workspace_root) = WtreeConfig::load()?;
    let worktrees_base = config.worktrees_path(&workspace_root);
    let slug_dir = worktrees_base.join(slug);

    if !slug_dir.exists() {
        anyhow::bail!(
            "Worktree \"{}\" not found.\n  Use `branchyard list` to see active worktrees.",
            slug
        );
    }

    // ── Dirty check — before any destructive operation ────────────────────────

    let mut dirty_repos: Vec<(&str, Vec<String>)> = Vec::new();

    for repo in &config.repos {
        let worktree_path = slug_dir.join(&repo.name);
        if !worktree_path.exists() {
            continue;
        }
        let changes = worktree::dirty_check(&worktree_path)?;
        if !changes.is_empty() {
            dirty_repos.push((&repo.name, changes));
        }
    }

    if !dirty_repos.is_empty() {
        println!("{}", "\nUncommitted changes detected:".yellow().bold());
        for (repo_name, changes) in &dirty_repos {
            println!(
                "  {} {} ({} file{})",
                "●".yellow(),
                repo_name.bold(),
                changes.len(),
                if changes.len() == 1 { "" } else { "s" }
            );
            for change in changes.iter().take(5) {
                println!("    {}", change.dimmed());
            }
            if changes.len() > 5 {
                println!("    {} more...", (changes.len() - 5).to_string().dimmed());
            }
        }

        let action = Select::new(
            "\nHow do you want to proceed?",
            vec![
                "Abort — go back and commit or stash your changes",
                "Discard — permanently lose all uncommitted changes",
            ],
        )
        .prompt()?;

        if action.starts_with("Abort") {
            println!("{} Aborted. Your changes are safe.", "✔".green().bold());
            return Ok(());
        }

        // User chose to discard — confirm once more
        let confirmed =
            Confirm::new("This will permanently discard all uncommitted changes. Are you sure?")
                .with_default(false)
                .prompt()?;

        if !confirmed {
            println!("{} Aborted. Your changes are safe.", "✔".green().bold());
            return Ok(());
        }
    }

    // ── Kill dev server processes running inside the worktree ─────────────────

    kill_processes_in_dir(&slug_dir);

    // ── Stop Docker services ──────────────────────────────────────────────────

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
    let vars = ports.variable_map(slug, &config.repos, &config.services);

    if !config.services.is_empty() && slug_dir.join("docker-compose.override.yml").exists() {
        println!("{}", "Stopping Docker services...".dimmed());
        docker::down(slug, &slug_dir).ok();
        println!("  {} Services stopped.", "✔".green().bold());
    }

    // ── Remove worktrees ──────────────────────────────────────────────────────

    for repo in &config.repos {
        let repo_path = config.repo_path(&repo.path, &workspace_root);
        let worktree_path = slug_dir.join(&repo.name);

        if worktree_path.exists() {
            worktree::remove(&repo_path, &worktree_path, true)?;
            println!(
                "  {} Worktree {} removed.",
                "✔".green().bold(),
                repo.name.bold()
            );
        }
    }

    // Remove the slug directory
    if slug_dir.exists() {
        std::fs::remove_dir_all(&slug_dir)?;
    }

    // ── Delete local branches ─────────────────────────────────────────────────

    // Read the original branch name (may differ from slug when --slug was used)
    let branch = std::fs::read_to_string(slug_dir.join(".branch"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| slug.to_string());

    let delete_branches = Confirm::new(&format!(
        "Delete local branch \"{}\" from all repos?",
        branch
    ))
    .with_default(true)
    .prompt()?;

    if delete_branches {
        for repo in &config.repos {
            let repo_path = config.repo_path(&repo.path, &workspace_root);
            match worktree::delete_branch(&repo_path, &branch) {
                Ok(_) => println!(
                    "  {} Branch {} deleted from {}.",
                    "✔".green().bold(),
                    branch.cyan(),
                    repo.name.bold()
                ),
                Err(e) => eprintln!(
                    "  {} Could not delete branch from {}: {}",
                    "✗".red().bold(),
                    repo.name.bold(),
                    e
                ),
            }
        }
    }

    // ── Terminal config + hooks ───────────────────────────────────────────────

    terminal::remove(slug, &config).ok();

    for hook in &config.hooks.after_done {
        let cmd = interpolate(hook, &vars);
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .status()?;
    }

    println!(
        "\n{} Worktree \"{}\" cleaned up.",
        "✔".green().bold(),
        slug.bold()
    );
    Ok(())
}
