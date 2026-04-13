use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Create a git worktree for a given repo at a target path on a new branch.
/// Uses the system `git` command (git2 crate used for reads; mutations via CLI for reliability).
pub fn create(repo_path: &Path, worktree_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            worktree_path
                .to_str()
                .expect("worktree path must be valid UTF-8"),
            "-b",
            branch,
        ])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    Ok(())
}

/// Remove a git worktree by its path.
pub fn remove(repo_path: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(
        worktree_path
            .to_str()
            .expect("worktree path must be valid UTF-8"),
    );

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_path)
        .output()
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove failed: {}", stderr.trim());
    }

    Ok(())
}

/// Check if a worktree directory has uncommitted changes.
/// Returns a list of (repo_name, dirty_files) for each repo with changes.
pub fn dirty_check(worktree_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        // If git status fails (e.g. not a git repo), treat as clean
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let dirty: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(dirty)
}

/// Delete a local branch from the repo.
pub fn delete_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["branch", "-d", branch])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git branch -d")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch -d failed: {}", stderr.trim());
    }

    Ok(())
}
