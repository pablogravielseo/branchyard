use anyhow::Result;
use owo_colors::OwoColorize;

use crate::config::WtreeConfig;
use crate::ports::PortAssignment;

pub fn run(json: bool) -> Result<()> {
    let (config, workspace_root) = WtreeConfig::load()?;
    let worktrees_base = config.worktrees_path(&workspace_root);

    if !worktrees_base.exists() {
        print_empty(json);
        return Ok(());
    }

    let mut slugs: Vec<String> = std::fs::read_dir(&worktrees_base)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();

    slugs.sort();

    if slugs.is_empty() {
        print_empty(json);
        return Ok(());
    }

    if json {
        let items: Vec<String> = slugs
            .iter()
            .map(|slug| {
                let slot = read_slot(&worktrees_base, slug);
                let branch = read_branch(&worktrees_base, slug);
                let ports = PortAssignment::new(
                    config.ports.base,
                    slot,
                    config.repos.len(),
                    config.services.len(),
                );
                let repo_ports: Vec<String> = config
                    .repos
                    .iter()
                    .zip(ports.repo_ports.iter())
                    .map(|(r, &p)| format!("\"{}\":{}", r.name, p))
                    .collect();
                format!(
                    "{{\"slug\":\"{slug}\",\"branch\":\"{branch}\",\"slot\":{slot},\"ports\":{{{}}}}}",
                    repo_ports.join(",")
                )
            })
            .collect();
        println!("[{}]", items.join(","));
        return Ok(());
    }

    // Human-readable table
    let slug_w = slugs.iter().map(|s| s.len()).max().unwrap_or(4).max(4);
    let branch_w = slugs
        .iter()
        .map(|s| read_branch(&worktrees_base, s).len())
        .max()
        .unwrap_or(6)
        .max(6);
    let slot_w = 4usize;

    // Header: SLUG | BRANCH | SLOT | <repo_name> per repo | <svc_name> per service
    print!(
        "{:<slug_w$}  {:<branch_w$}  {:<slot_w$}",
        "SLUG".bold(),
        "BRANCH".bold(),
        "SLOT".bold()
    );
    for repo in &config.repos {
        print!("  {:<8}", repo.name.to_uppercase().bold());
    }
    for svc in &config.services {
        print!("  {:<8}", svc.name.to_uppercase().bold());
    }
    println!();

    let sep_len =
        slug_w + 2 + branch_w + 2 + slot_w + (config.repos.len() + config.services.len()) * 10;
    println!("{}", "─".repeat(sep_len).dimmed());

    for slug in &slugs {
        let slot = read_slot(&worktrees_base, slug);
        let branch = read_branch(&worktrees_base, slug);
        let ports = PortAssignment::new(
            config.ports.base,
            slot,
            config.repos.len(),
            config.services.len(),
        );

        print!(
            "{:<slug_w$}  {:<branch_w$}  {:<slot_w$}",
            slug, branch, slot
        );
        for port in &ports.repo_ports {
            print!("  {:<8}", port);
        }
        for port in &ports.service_ports {
            print!("  {:<8}", port);
        }
        println!();
    }

    Ok(())
}

fn read_slot(worktrees_base: &std::path::Path, slug: &str) -> u16 {
    std::fs::read_to_string(worktrees_base.join(slug).join(".slot"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn read_branch(worktrees_base: &std::path::Path, slug: &str) -> String {
    std::fs::read_to_string(worktrees_base.join(slug).join(".branch"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| slug.to_string())
}

fn print_empty(json: bool) {
    if json {
        println!("[]");
    } else {
        println!("No active worktrees.");
        println!(
            "  Run {} to create one.",
            "`branchyard new <branch>`".bold()
        );
    }
}
