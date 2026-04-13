use anyhow::Result;
use owo_colors::OwoColorize;

use crate::config::WtreeConfig;
use crate::docker;

pub fn run(slug: &str) -> Result<()> {
    let (config, workspace_root) = WtreeConfig::load()?;
    let slug_dir = config.worktrees_path(&workspace_root).join(slug);

    if !slug_dir.exists() {
        anyhow::bail!(
            "Worktree \"{}\" not found.\n  Use `branchyard list` to see active worktrees.",
            slug
        );
    }

    if !config.services.is_empty() {
        println!(
            "{}",
            format!("Stopping services for \"{}\"...", slug).dimmed()
        );
        docker::down(slug, &slug_dir)?;
        println!("  {} Docker services stopped.", "✔".green().bold());
    }

    // Note: dev servers launched by `branchyard serve` run as detached processes.
    // Users can stop them from within their Warp tabs or via their own stop commands.
    if config.repos.iter().any(|r| r.commands.stop.is_some()) {
        println!(
            "{}",
            "Note: stop dev servers from within their terminal tabs.".dimmed()
        );
    }

    println!("{} Done.", "✔".green().bold());
    Ok(())
}
