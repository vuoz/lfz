use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Represents a west.yml manifest file
#[derive(Debug, Deserialize)]
pub struct WestManifest {
    pub manifest: ManifestContent,
}

#[derive(Debug, Deserialize)]
pub struct ManifestContent {
    #[serde(default)]
    pub remotes: Vec<Remote>,

    #[serde(default)]
    pub projects: Vec<Project>,

    #[serde(rename = "self")]
    pub self_config: Option<SelfConfig>,
}

#[derive(Debug, Deserialize)]
pub struct Remote {
    pub name: String,

    #[serde(rename = "url-base")]
    pub url_base: String,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub name: String,

    #[serde(default)]
    pub remote: Option<String>,

    #[serde(default)]
    pub revision: Option<String>,

    #[serde(default)]
    pub path: Option<String>,

    /// Import another west.yml from this project
    #[serde(rename = "import")]
    pub import_path: Option<ImportConfig>,
}

/// Import can be a string path or an object with more options
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ImportConfig {
    Simple(String),
    Complex {
        #[serde(default)]
        file: Option<String>,
        #[serde(rename = "name-blocklist")]
        name_blocklist: Option<Vec<String>>,
        #[serde(rename = "path-blocklist")]
        path_blocklist: Option<Vec<String>>,
    },
}

#[derive(Debug, Deserialize)]
pub struct SelfConfig {
    #[serde(default)]
    pub path: Option<String>,
}

impl WestManifest {
    /// Load west.yml from a path
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read west.yml at {}", path.display()))?;

        serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse west.yml at {}", path.display()))
    }

    /// Get the ZMK project if present
    pub fn zmk_project(&self) -> Option<&Project> {
        self.manifest.projects.iter().find(|p| p.name == "zmk")
    }

    /// Get the ZMK revision (branch, tag, or commit)
    pub fn zmk_revision(&self) -> Option<&str> {
        self.zmk_project().and_then(|p| p.revision.as_deref())
    }

    /// Get the self path (where the config is located)
    pub fn self_path(&self) -> Option<&str> {
        self.manifest
            .self_config
            .as_ref()
            .and_then(|s| s.path.as_deref())
    }

    /// Get remote URL by name
    pub fn remote_url(&self, name: &str) -> Option<String> {
        self.manifest
            .remotes
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.url_base.clone())
    }

    /// Get full URL for a project
    pub fn project_url(&self, project: &Project) -> Option<String> {
        let remote_name = project.remote.as_ref()?;
        let base_url = self.remote_url(remote_name)?;
        Some(format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            project.name
        ))
    }
}

/// Compute a hash of the west.yml file content for cache keying (legacy)
pub fn hash_west_yml(path: &Path) -> Result<String> {
    let content =
        fs::read(path).with_context(|| format!("Failed to read west.yml at {}", path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();

    // Return first 16 chars of hex for a shorter but still unique key
    Ok(hex::encode(&result[..8]))
}

/// Get git repository info for cache keying
/// Returns (remote_url or repo_path, branch_or_commit)
pub fn get_git_info(config_dir: &Path) -> Result<(String, String)> {
    use std::process::Command;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_west_yml() {
        let yaml = r#"
manifest:
  remotes:
    - name: zmkfirmware
      url-base: https://github.com/zmkfirmware
  projects:
    - name: zmk
      remote: zmkfirmware
      revision: main
      import: app/west.yml
  self:
    path: config
"#;
        let manifest: WestManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.manifest.remotes.len(), 1);
        assert_eq!(manifest.manifest.projects.len(), 1);
        assert_eq!(manifest.zmk_revision(), Some("main"));
        assert_eq!(manifest.self_path(), Some("config"));
    }

    #[test]
    fn test_parse_complex_import() {
        let yaml = r#"
manifest:
  remotes:
    - name: zmkfirmware
      url-base: https://github.com/zmkfirmware
  projects:
    - name: zmk
      remote: zmkfirmware
      revision: main
      import:
        file: app/west.yml
        name-blocklist:
          - ci-tools
"#;
        let manifest: WestManifest = serde_yaml::from_str(yaml).unwrap();
        let zmk = manifest.zmk_project().unwrap();
        match &zmk.import_path {
            Some(ImportConfig::Complex { name_blocklist, .. }) => {
                assert!(name_blocklist.is_some());
            }
            _ => panic!("Expected complex import"),
        }
    }

    #[test]
    fn test_project_url() {
        let yaml = r#"
manifest:
  remotes:
    - name: zmkfirmware
      url-base: https://github.com/zmkfirmware
  projects:
    - name: zmk
      remote: zmkfirmware
      revision: main
"#;
        let manifest: WestManifest = serde_yaml::from_str(yaml).unwrap();
        let zmk = manifest.zmk_project().unwrap();
        let url = manifest.project_url(zmk).unwrap();
        assert_eq!(url, "https://github.com/zmkfirmware/zmk");
    }
}
