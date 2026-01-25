use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

/// Get git repository info for cache keying
/// Returns (remote_url or repo_path, branch_or_commit)
pub fn get_git_info(config_dir: &Path) -> Result<(String, String)> {
    // Get the repo root (config_dir might be a subdirectory)
    let repo_root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(config_dir)
        .output()
        .context("Failed to run git rev-parse")?;

    let repo_root = if repo_root.status.success() {
        String::from_utf8_lossy(&repo_root.stdout)
            .trim()
            .to_string()
    } else {
        // Not a git repo, use the config directory path
        return Ok((
            config_dir.canonicalize()?.to_string_lossy().to_string(),
            "default".to_string(),
        ));
    };

    // Try to get the remote URL (prefer 'origin')
    let remote_url = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Fall back to first remote, or repo path if no remotes
    let repo_id = remote_url.unwrap_or_else(|| {
        // Try to get any remote
        Command::new("git")
            .args(["remote"])
            .current_dir(&repo_root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let remotes = String::from_utf8_lossy(&o.stdout);
                let first_remote = remotes.lines().next()?;
                Command::new("git")
                    .args(["remote", "get-url", first_remote])
                    .current_dir(&repo_root)
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
            .unwrap_or_else(|| repo_root.clone())
    });

    // Get current branch or commit
    let branch = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let branch_or_commit = branch.unwrap_or_else(|| {
        // Detached HEAD - use commit SHA
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&repo_root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    });

    Ok((repo_id, branch_or_commit))
}

/// Compute a workspace hash based on git repo + branch
pub fn hash_workspace_key(config_dir: &Path) -> Result<String> {
    let (repo_id, branch) = get_git_info(config_dir)?;
    let key = format!("{}:{}", repo_id, branch);

    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let result = hasher.finalize();

    // Return first 16 chars of hex
    Ok(hex::encode(&result[..8]))
}
