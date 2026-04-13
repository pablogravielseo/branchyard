//! Warp terminal integration.
//!
//! Writes a Launch Configuration YAML to:
//!   ~/Documents/Warp/Launch Configurations/branchyard-<slug>.yaml
//!
//! Warp picks it up automatically. The user opens it via CMD+P → "branchyard · <slug>".
//! Each repo gets its own tab, pre-cd'd to the worktree directory.
//! If `commands.serve` is defined, it runs automatically when the tab opens.

use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::Path;
use std::process::Command;

use crate::config::WtreeConfig;
use crate::ports::{interpolate, PortAssignment};
use crate::terminal::TerminalIntegration;

pub struct Warp;

impl TerminalIntegration for Warp {
    fn open(
        &self,
        slug: &str,
        config: &WtreeConfig,
        worktree_base: &Path,
        ports: &PortAssignment,
        vars: &HashMap<String, String>,
    ) -> Result<()> {
        let content = generate(slug, config, worktree_base, ports, vars);
        let path = config_path(slug);
        std::fs::create_dir_all(
            path.parent()
                .expect("config path always has a parent directory"),
        )?;
        std::fs::write(&path, &content)?;
        Command::new("open").arg("-a").arg("Warp").status().ok();
        Ok(())
    }

    fn remove(&self, slug: &str) -> Result<()> {
        let path = config_path(slug);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn config_path(slug: &str) -> std::path::PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Warp")
        .join("Launch Configurations")
        .join(format!("branchyard-{slug}.yaml"))
}

fn generate(
    slug: &str,
    config: &WtreeConfig,
    worktree_base: &Path,
    ports: &PortAssignment,
    vars: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: branchyard · {slug}");
    let _ = writeln!(out, "tabs:");

    for (i, repo) in config.repos.iter().enumerate() {
        let path = worktree_base.join(slug).join(&repo.name);
        let _ = writeln!(out, "  - title: \"{} · {slug}\"", repo.name);
        let _ = writeln!(out, "    directory: {}", path.display());

        if let Some(serve_cmd) = &repo.commands.serve {
            let mut repo_vars = vars.clone();
            if let Some(&port) = ports.repo_ports.get(i) {
                repo_vars.insert("port".to_string(), port.to_string());
            }
            let cmd = interpolate(serve_cmd, &repo_vars);
            let _ = writeln!(out, "    commands:");
            let _ = writeln!(out, "      - {cmd}");
        }
    }

    out
}
