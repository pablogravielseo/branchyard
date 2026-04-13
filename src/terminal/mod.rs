//! Terminal integration layer.
//!
//! branchyard can open a pre-configured terminal session on `branchyard new` and clean
//! it up on `branchyard done`. The specific terminal is declared in .branchyard.yml:
//!
//! ```yaml
//! terminal:
//!   multiplexer: warp   # warp | iterm2 | tmux | ghostty | none
//! ```
//!
//! ## Adding a new terminal
//!
//! 1. Create `src/terminal/<name>.rs` and implement the `TerminalIntegration` trait.
//! 2. Add a variant to `config::Multiplexer`.
//! 3. Add the match arm in `resolve()` below.
//! 4. That's it — no other files need to change.
//!
//! The trait contract:
//!   - `open`   → called by `branchyard new`. Write config, launch terminal.
//!   - `remove` → called by `branchyard done`. Delete config file if any.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::{Multiplexer, WtreeConfig};
use crate::ports::PortAssignment;

mod ghostty;
mod iterm2;
mod tmux;
mod warp;

/// The contract every terminal integration must fulfill.
pub trait TerminalIntegration {
    /// Open a terminal session for the given worktree.
    /// Called once by `branchyard new` after worktrees and services are ready.
    fn open(
        &self,
        slug: &str,
        config: &WtreeConfig,
        worktree_base: &Path,
        ports: &PortAssignment,
        vars: &HashMap<String, String>,
    ) -> Result<()>;

    /// Remove any config files created by `open`.
    /// Called by `branchyard done`. Should be a no-op if nothing was created.
    fn remove(&self, slug: &str) -> Result<()>;
}

/// Resolve the configured multiplexer to its integration implementation.
fn resolve(multiplexer: &Multiplexer) -> Option<Box<dyn TerminalIntegration>> {
    match multiplexer {
        Multiplexer::None => None,
        Multiplexer::Warp => Some(Box::new(warp::Warp)),
        Multiplexer::Iterm2 => Some(Box::new(iterm2::Iterm2)),
        Multiplexer::Tmux => Some(Box::new(tmux::Tmux)),
        Multiplexer::Ghostty => Some(Box::new(ghostty::Ghostty)),
    }
}

/// Open a terminal session. No-op if multiplexer is None.
pub fn open(
    slug: &str,
    config: &WtreeConfig,
    worktree_base: &Path,
    ports: &PortAssignment,
    vars: &HashMap<String, String>,
) -> Result<()> {
    if let Some(integration) = resolve(&config.terminal.multiplexer) {
        integration.open(slug, config, worktree_base, ports, vars)?;
    }
    Ok(())
}

/// Open a terminal session only when autostart is enabled.
/// Used by `branchyard serve` to optionally launch dev servers.
pub fn open_if_autostart(
    slug: &str,
    config: &WtreeConfig,
    worktree_base: &Path,
    ports: &PortAssignment,
    vars: &HashMap<String, String>,
) -> Result<()> {
    if config.terminal.autostart {
        open(slug, config, worktree_base, ports, vars)?;
    }
    Ok(())
}

/// Remove terminal config. No-op if multiplexer is None.
pub fn remove(slug: &str, config: &WtreeConfig) -> Result<()> {
    if let Some(integration) = resolve(&config.terminal.multiplexer) {
        integration.remove(slug)?;
    }
    Ok(())
}
