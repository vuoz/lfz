use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::PathBuf;

/// Get the cache directory for lfz
/// Uses platform-appropriate location:
/// - Linux: ~/.cache/lfz
/// - macOS: ~/Library/Caches/lfz
/// - Windows: C:\Users\<user>\AppData\Local\lfz\cache
pub fn cache_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("", "", "lfz")
        .context("Could not determine cache directory for your platform")?;

    Ok(proj_dirs.cache_dir().to_path_buf())
}

/// Get the directory where west workspaces are cached
pub fn workspaces_dir() -> Result<PathBuf> {
    Ok(cache_dir()?.join("workspaces"))
}

/// Get the shared ccache directory
pub fn ccache_dir() -> Result<PathBuf> {
    Ok(cache_dir()?.join("ccache"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_dir_exists() {
        let dir = cache_dir().unwrap();
        assert!(dir.to_string_lossy().contains("lfz"));
    }
}
