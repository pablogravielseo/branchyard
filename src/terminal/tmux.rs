//! Tmux terminal integration.
//!
//! Creates a named tmux session: branchyard-<slug>
//! One window per repo, each pre-cd'd to the worktree directory.
//! If `commands.serve` is defined, it runs in that window automatically.
//!
//! Requires: tmux installed and available on $PATH.
//! Works on macOS and Linux — no config files written.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::config::WtreeConfig;
use crate::ports::{interpolate, PortAssignment};
use crate::terminal::TerminalIntegration;

pub struct Tmux;

const SESSION_PREFIX: &str = "branchyard-";

impl TerminalIntegration for Tmux {
    fn open(
        &self,
        slug: &str,
        config: &WtreeConfig,
        worktree_base: &Path,
        ports: &PortAssignment,
        vars: &HashMap<String, String>,
    ) -> Result<()> {
        let session = format!("{SESSION_PREFIX}{slug}");

        // Create a detached session named after the slug
        Command::new("tmux")
            .args(["new-session", "-d", "-s", &session])
            .status()
            .context("Failed to create tmux session. Is tmux installed?")?;

        for (i, repo) in config.repos.iter().enumerate() {
            let path = worktree_base.join(slug).join(&repo.name);
            let window_name = format!("{} · {slug}", repo.name);

            if i == 0 {
                // Rename the default window instead of creating a new one
                Command::new("tmux")
                    .args(["rename-window", "-t", &format!("{session}:0"), &window_name])
                    .status()
                    .ok();
            } else {
                // Create additional windows
                Command::new("tmux")
                    .args(["new-window", "-t", &session, "-n", &window_name])
                    .status()
                    .ok();
            }

            // cd into the worktree directory
            Command::new("tmux")
                .args([
                    "send-keys",
                    "-t",
                    &format!("{session}:{i}"),
                    &format!("cd {} && clear", path.display()),
                    "Enter",
                ])
                .status()
                .ok();

            // Run serve command if defined
            if let Some(serve_cmd) = &repo.commands.serve {
                let mut repo_vars = vars.clone();
                if let Some(&port) = ports.repo_ports.get(i) {
                    repo_vars.insert("port".to_string(), port.to_string());
                }
                let cmd = interpolate(serve_cmd, &repo_vars);
                Command::new("tmux")
                    .args(["send-keys", "-t", &format!("{session}:{i}"), &cmd, "Enter"])
                    .status()
                    .ok();
            }
        }

        // Attach to the session (switches the current terminal into tmux)
        Command::new("tmux")
            .args(["attach-session", "-t", &session])
            .status()
            .ok();

        Ok(())
    }

    fn remove(&self, slug: &str) -> Result<()> {
        let session = format!("{SESSION_PREFIX}{slug}");

        // Kill the session if it exists — ignore errors (session may already be gone)
        Command::new("tmux")
            .args(["kill-session", "-t", &session])
            .status()
            .ok();

        Ok(())
    }
}
