//! iTerm2 terminal integration.
//!
//! Writes a Dynamic Profile JSON to:
//!   ~/Library/Application Support/iTerm2/DynamicProfiles/branchyard-<slug>.json
//!
//! iTerm2 loads Dynamic Profiles automatically at runtime — no restart needed.
//! Each repo gets its own profile (window/tab), pre-cd'd to the worktree directory.
//! If `commands.serve` is defined, it runs as the initial command for that tab.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::config::WtreeConfig;
use crate::ports::{interpolate, PortAssignment};
use crate::terminal::TerminalIntegration;

pub struct Iterm2;

impl TerminalIntegration for Iterm2 {
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
        Command::new("open").arg("-a").arg("iTerm").status().ok();
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
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library")
        .join("Application Support")
        .join("iTerm2")
        .join("DynamicProfiles")
        .join(format!("branchyard-{slug}.json"))
}

fn generate(
    slug: &str,
    config: &WtreeConfig,
    worktree_base: &Path,
    ports: &PortAssignment,
    vars: &HashMap<String, String>,
) -> String {
    let profiles: Vec<String> = config
        .repos
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let path = worktree_base.join(slug).join(&repo.name);

            let initial_cmd = repo.commands.serve.as_ref().map(|serve_cmd| {
                let mut repo_vars = vars.clone();
                if let Some(&port) = ports.repo_ports.get(i) {
                    repo_vars.insert("port".to_string(), port.to_string());
                }
                interpolate(serve_cmd, &repo_vars)
            });

            let cmd_field = match &initial_cmd {
                Some(cmd) => format!(
                    "      \"Initial Text\": \"{}\",\n",
                    cmd.replace('\\', "\\\\").replace('"', "\\\"")
                ),
                None => String::new(),
            };

            format!(
                "    {{\n      \"Name\": \"{repo} · {slug}\",\n      \"Guid\": \"branchyard-{slug}-{repo}\",\n      \"Working Directory\": \"{dir}\",\n{cmd}      \"Custom Directory\": \"Yes\"\n    }}",
                repo = repo.name,
                dir = path.display(),
                cmd = cmd_field,
            )
        })
        .collect();

    format!("{{\n  \"Profiles\": [\n{}\n  ]\n}}", profiles.join(",\n"))
}
