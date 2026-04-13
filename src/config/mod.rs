use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = ".wtree.yml";

#[derive(Debug, Serialize, Deserialize)]
pub struct WtreeConfig {
    pub base_branch: String,
    #[serde(default = "default_worktrees_dir")]
    pub worktrees_dir: String,
    pub repos: Vec<RepoConfig>,
    pub ports: PortConfig,
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
}

fn default_worktrees_dir() -> String {
    "./worktrees".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoConfig {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub commands: RepoCommands,
    #[serde(default)]
    pub setup: SetupConfig,
}

/// Shell commands the CLI delegates to for each lifecycle event.
///
/// Supported placeholders (interpolated at runtime):
///   {slug}           — worktree name
///   {port}           — this repo's assigned host port
///   {<name>_port}    — port of any repo or service by name
///                      e.g. {backend_port}, {postgres_port}, {redis_port}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RepoCommands {
    /// Start the dev server. Runs detached from within the worktree directory.
    pub serve: Option<String>,
    /// Stop the dev server. If absent, the user stops it manually in the terminal.
    pub stop: Option<String>,
    /// One-time setup on `branchyard new` (migrations, installs, restores, etc.).
    /// Runs synchronously from within the worktree directory before handing off.
    pub setup: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SetupConfig {
    /// Symlinks to create inside the worktree on `branchyard new`.
    /// `from` — path relative to the workspace root (where .wtree.yml lives).
    /// `to`   — path relative to the repo's worktree directory.
    #[serde(default)]
    pub symlinks: Vec<SymlinkConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SymlinkConfig {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortConfig {
    /// All ports for a worktree slot are derived from this base.
    /// slot 0 → base+0 .. base+N
    /// slot 1 → base+10 .. base+10+N
    pub base: u16,
}

/// A Docker service managed by branchyard via a standalone docker-compose.yml.
/// The container port is fixed; the host port is assigned per slot automatically.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub image: String,
    /// Fixed container-side port (e.g. 5432 for Postgres, 6379 for Redis).
    /// Use 0 for services that don't expose a port (e.g. background workers).
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub environment: Vec<String>,
    /// Shell command to run inside the container (overrides image default).
    #[serde(default)]
    pub command: Option<String>,
    /// Services this service depends on (by name).
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Volumes to mount: "host_path:container_path" or named volume.
    #[serde(default)]
    pub volumes: Vec<String>,
    /// Path to an env_file relative to the repo worktree.
    #[serde(default)]
    pub env_file: Option<String>,
    /// Platform override (e.g. "linux/amd64").
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TerminalConfig {
    /// Terminal multiplexer to integrate with.
    /// Supported: "warp", "iterm2", "none" (default)
    /// "none" — branchyard skips terminal setup entirely.
    #[serde(default)]
    pub multiplexer: Multiplexer,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Multiplexer {
    /// No terminal integration. branchyard skips this step entirely.
    #[default]
    None,
    /// Warp — writes a Launch Configuration YAML.
    /// Config dir: ~/Documents/Warp/Launch Configurations/
    Warp,
    /// iTerm2 — writes a Dynamic Profile JSON.
    /// Config dir: ~/Library/Application Support/iTerm2/DynamicProfiles/
    Iterm2,
    /// Tmux — creates a named session with one window per repo.
    /// Requires tmux to be installed and available on $PATH.
    Tmux,
    /// Ghostty — writes a config file fragment.
    /// Config dir: ~/.config/ghostty/
    Ghostty,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub after_new: Vec<String>,
    #[serde(default)]
    pub after_done: Vec<String>,
}

impl WtreeConfig {
    pub fn load() -> Result<(Self, PathBuf)> {
        let config_path = find_config_file().ok_or_else(|| {
            anyhow::anyhow!(
                ".wtree.yml not found in this directory or any parent.\n  Run `branchyard init` to configure this workspace."
            )
        })?;

        let workspace_root = config_path
            .parent()
            .expect(".wtree.yml must have a parent directory")
            .to_path_buf()
            .canonicalize()
            .with_context(|| {
                format!(
                    "Failed to resolve workspace root from {}",
                    config_path.display()
                )
            })?;

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid .wtree.yml at {}", config_path.display()))?;

        Ok((config, workspace_root))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Absolute path to the worktrees directory, resolved from the workspace root.
    pub fn worktrees_path(&self, workspace_root: &Path) -> PathBuf {
        let p = PathBuf::from(&self.worktrees_dir);
        if p.is_absolute() {
            p
        } else {
            workspace_root.join(p)
        }
    }

    /// Absolute path to a repo, resolved from the workspace root.
    pub fn repo_path(&self, repo_path: &str, workspace_root: &Path) -> PathBuf {
        let p = PathBuf::from(repo_path);
        if p.is_absolute() {
            p
        } else {
            workspace_root.join(p)
        }
    }
}

fn find_config_file() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(CONFIG_FILE);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
