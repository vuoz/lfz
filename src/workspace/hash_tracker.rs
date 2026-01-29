//! Tracks build configuration hashes to determine if incremental builds are safe.
//!
//! When build.yaml or west.yml change, incremental builds may have stale artifacts.
//! This module tracks hashes of these files to automatically decide whether to use
//! pristine (safe) or incremental (fast) builds.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// File name for storing build hashes in the workspace
const HASH_FILE: &str = ".lfz_build_hashes.json";

/// Hashes of configuration files that affect build output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildHashes {
    /// SHA256 hash of build.yaml contents
    pub build_yaml: String,
    /// SHA256 hash of west.yml contents  
    pub west_yml: String,
}

impl BuildHashes {
    /// Calculate hashes from the project's configuration files
    pub fn calculate(build_yaml_path: &Path, west_yml_path: &Path) -> Result<Self> {
        let build_yaml_hash = hash_file(build_yaml_path)
            .with_context(|| format!("Failed to hash {}", build_yaml_path.display()))?;

        let west_yml_hash = hash_file(west_yml_path)
            .with_context(|| format!("Failed to hash {}", west_yml_path.display()))?;

        Ok(Self {
            build_yaml: build_yaml_hash,
            west_yml: west_yml_hash,
        })
    }

    /// Load previously stored hashes from a workspace
    pub fn load(workspace: &Path) -> Result<Option<Self>> {
        let hash_file = workspace.join(HASH_FILE);

        if !hash_file.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&hash_file)
            .with_context(|| format!("Failed to read {}", hash_file.display()))?;

        let hashes: Self = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse {}", hash_file.display()))?;

        Ok(Some(hashes))
    }

    /// Save hashes to a workspace for future comparison
    pub fn save(&self, workspace: &Path) -> Result<()> {
        let hash_file = workspace.join(HASH_FILE);

        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize build hashes")?;

        fs::write(&hash_file, contents)
            .with_context(|| format!("Failed to write {}", hash_file.display()))?;

        Ok(())
    }

    /// Check if these hashes match stored hashes, indicating incremental build is safe
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

/// Determine if incremental build is safe based on current vs stored hashes
pub fn is_incremental_safe(workspace: &Path, current: &BuildHashes) -> bool {
    match BuildHashes::load(workspace) {
        Ok(Some(stored)) => current.matches(&stored),
        Ok(None) => false, // No stored hashes = first build, use pristine
        Err(_) => false,   // Error reading = be safe, use pristine
    }
}

/// Calculate SHA256 hash of a file's contents
fn hash_file(path: &Path) -> Result<String> {
    let contents =
        fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_hash_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello world").unwrap();

        let hash = hash_file(&file).unwrap();
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex chars
    }

    #[test]
    fn test_hash_file_deterministic() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello world").unwrap();

        let hash1 = hash_file(&file).unwrap();
        let hash2 = hash_file(&file).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_build_hashes_calculate() {
        let dir = tempdir().unwrap();
        let build_yaml = dir.path().join("build.yaml");
        let west_yml = dir.path().join("west.yml");

        fs::write(&build_yaml, "board: [nice_nano_v2]").unwrap();
        fs::write(&west_yml, "manifest:\n  projects: []").unwrap();

        let hashes = BuildHashes::calculate(&build_yaml, &west_yml).unwrap();
        assert!(!hashes.build_yaml.is_empty());
        assert!(!hashes.west_yml.is_empty());
    }

    #[test]
    fn test_build_hashes_save_load() {
        let dir = tempdir().unwrap();
        let workspace = dir.path();

        let hashes = BuildHashes {
            build_yaml: "abc123".to_string(),
            west_yml: "def456".to_string(),
        };

        hashes.save(workspace).unwrap();
        let loaded = BuildHashes::load(workspace).unwrap();
        assert_eq!(loaded, Some(hashes));
    }

    #[test]
    fn test_build_hashes_load_missing() {
        let dir = tempdir().unwrap();
        let loaded = BuildHashes::load(dir.path()).unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn test_is_incremental_safe_no_stored() {
        let dir = tempdir().unwrap();
        let current = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
        };

        assert!(!is_incremental_safe(dir.path(), &current));
    }

    #[test]
    fn test_is_incremental_safe_matches() {
        let dir = tempdir().unwrap();
        let hashes = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
        };

        hashes.save(dir.path()).unwrap();
        assert!(is_incremental_safe(dir.path(), &hashes));
    }

    #[test]
    fn test_is_incremental_safe_different() {
        let dir = tempdir().unwrap();
        let stored = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
        };
        stored.save(dir.path()).unwrap();

        let current = BuildHashes {
            build_yaml: "xyz".to_string(), // Changed!
            west_yml: "def".to_string(),
        };
        assert!(!is_incremental_safe(dir.path(), &current));
    }
}
