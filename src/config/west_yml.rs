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

/// Format git info as "project:branch" for display
/// Extracts just the repo name from URLs like:
/// - https://github.com/user/repo.git -> repo
/// - git@github.com:user/repo.git -> repo
/// - /path/to/local/repo -> repo
pub fn format_project_display(config_dir: &Path) -> Result<String> {
    let (repo_id, branch) = get_git_info(config_dir)?;

    // Extract repo name from URL or path
    let repo_name = extract_repo_name(&repo_id);

    Ok(format!("{}:{}", repo_name, branch))
}

/// Extract repository name from a git URL or path
fn extract_repo_name(repo_id: &str) -> String {
    // Remove trailing .git if present
    let cleaned = repo_id.trim_end_matches(".git");

    // Try to extract from URL patterns
    // https://github.com/user/repo or git@github.com:user/repo
    if let Some(name) = cleaned.rsplit('/').next() {
        if !name.is_empty() {
            return name.to_string();
        }
    }

    // Handle git@host:user/repo format
    if let Some(name) = cleaned
        .rsplit(':')
        .next()
        .and_then(|s| s.rsplit('/').next())
    {
        if !name.is_empty() {
            return name.to_string();
        }
    }

    // Fallback: use the whole string if we can't parse it
    cleaned.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_repo_name_https() {
        assert_eq!(
            extract_repo_name("https://github.com/user/my-keyboard.git"),
            "my-keyboard"
        );
        assert_eq!(
            extract_repo_name("https://github.com/user/my-keyboard"),
            "my-keyboard"
        );
    }

    #[test]
    fn test_extract_repo_name_ssh() {
        assert_eq!(
            extract_repo_name("git@github.com:user/my-keyboard.git"),
            "my-keyboard"
        );
        assert_eq!(
            extract_repo_name("git@github.com:user/my-keyboard"),
            "my-keyboard"
        );
    }

    #[test]
    fn test_extract_repo_name_local_path() {
        assert_eq!(
            extract_repo_name("/Users/someone/projects/my-keyboard"),
            "my-keyboard"
        );
        assert_eq!(extract_repo_name("/home/user/zmk-config"), "zmk-config");
    }
}
