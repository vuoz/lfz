//! Tracks build configuration hashes to determine if incremental builds are safe.
//!
//! When build.yaml, west.yml, or custom board/shield definitions change,
//! incremental builds may have stale artifacts. This module tracks hashes
//! of these files to automatically decide whether to use pristine (safe)
//! or incremental (fast) builds.

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
    /// SHA256 hash of boards/ directory contents (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boards_dir: Option<String>,
    /// SHA256 hash of shields/ directory contents (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shields_dir: Option<String>,
}

impl BuildHashes {
    /// Calculate hashes from the project's configuration files and directories
    pub fn calculate(
        project_root: &Path,
        build_yaml_path: &Path,
        west_yml_path: &Path,
    ) -> Result<Self> {
        let build_yaml_hash = hash_file(build_yaml_path)
            .with_context(|| format!("Failed to hash {}", build_yaml_path.display()))?;

        let west_yml_hash = hash_file(west_yml_path)
            .with_context(|| format!("Failed to hash {}", west_yml_path.display()))?;

        // Hash custom board/shield directories if they exist
        let boards_dir = project_root.join("boards");
        let boards_hash = if boards_dir.is_dir() {
            Some(hash_directory(&boards_dir)?)
        } else {
            None
        };

        let shields_dir = project_root.join("shields");
        let shields_hash = if shields_dir.is_dir() {
            Some(hash_directory(&shields_dir)?)
        } else {
            None
        };

        Ok(Self {
            build_yaml: build_yaml_hash,
            west_yml: west_yml_hash,
            boards_dir: boards_hash,
            shields_dir: shields_hash,
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

/// Calculate SHA256 hash of a directory's contents (recursively)
///
/// Hashes all files in the directory, sorted by path for determinism.
/// The hash includes both file paths (relative to dir) and contents.
fn hash_directory(dir: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut files: Vec<_> = collect_files(dir)?;

    // Sort for deterministic ordering
    files.sort();

    for file_path in files {
        // Include relative path in hash (so renames are detected)
        let relative = file_path.strip_prefix(dir).unwrap_or(&file_path);
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update(b"\0"); // separator

        // Include file contents
        let contents = fs::read(&file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        hasher.update(&contents);
        hasher.update(b"\0"); // separator
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Recursively collect all files in a directory
fn collect_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if !dir.is_dir() {
        return Ok(files);
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            files.extend(collect_files(&path)?);
        } else if path.is_file() {
            files.push(path);
        }
    }

    Ok(files)
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
        let root = dir.path();
        let build_yaml = root.join("build.yaml");
        let west_yml = root.join("west.yml");

        fs::write(&build_yaml, "board: [nice_nano_v2]").unwrap();
        fs::write(&west_yml, "manifest:\n  projects: []").unwrap();

        let hashes = BuildHashes::calculate(root, &build_yaml, &west_yml).unwrap();
        assert!(!hashes.build_yaml.is_empty());
        assert!(!hashes.west_yml.is_empty());
        assert!(hashes.boards_dir.is_none()); // No boards/ dir
        assert!(hashes.shields_dir.is_none()); // No shields/ dir
    }

    #[test]
    fn test_build_hashes_with_boards_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let build_yaml = root.join("build.yaml");
        let west_yml = root.join("west.yml");
        let boards_dir = root.join("boards");

        fs::write(&build_yaml, "board: [my_board]").unwrap();
        fs::write(&west_yml, "manifest:\n  projects: []").unwrap();
        fs::create_dir(&boards_dir).unwrap();
        fs::write(boards_dir.join("my_board.conf"), "CONFIG_FOO=y").unwrap();

        let hashes = BuildHashes::calculate(root, &build_yaml, &west_yml).unwrap();
        assert!(hashes.boards_dir.is_some());
        assert!(hashes.shields_dir.is_none());
    }

    #[test]
    fn test_build_hashes_detects_board_changes() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let build_yaml = root.join("build.yaml");
        let west_yml = root.join("west.yml");
        let boards_dir = root.join("boards");

        fs::write(&build_yaml, "board: [my_board]").unwrap();
        fs::write(&west_yml, "manifest:\n  projects: []").unwrap();
        fs::create_dir(&boards_dir).unwrap();
        fs::write(boards_dir.join("my_board.conf"), "CONFIG_FOO=y").unwrap();

        let hashes1 = BuildHashes::calculate(root, &build_yaml, &west_yml).unwrap();

        // Modify board config
        fs::write(boards_dir.join("my_board.conf"), "CONFIG_FOO=n").unwrap();
        let hashes2 = BuildHashes::calculate(root, &build_yaml, &west_yml).unwrap();

        assert_ne!(hashes1.boards_dir, hashes2.boards_dir);
    }

    #[test]
    fn test_build_hashes_save_load() {
        let dir = tempdir().unwrap();
        let workspace = dir.path();

        let hashes = BuildHashes {
            build_yaml: "abc123".to_string(),
            west_yml: "def456".to_string(),
            boards_dir: Some("boards789".to_string()),
            shields_dir: None,
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
            boards_dir: None,
            shields_dir: None,
        };

        assert!(!is_incremental_safe(dir.path(), &current));
    }

    #[test]
    fn test_is_incremental_safe_matches() {
        let dir = tempdir().unwrap();
        let hashes = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
            boards_dir: None,
            shields_dir: None,
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
            boards_dir: None,
            shields_dir: None,
        };
        stored.save(dir.path()).unwrap();

        let current = BuildHashes {
            build_yaml: "xyz".to_string(), // Changed!
            west_yml: "def".to_string(),
            boards_dir: None,
            shields_dir: None,
        };
        assert!(!is_incremental_safe(dir.path(), &current));
    }

    #[test]
    fn test_is_incremental_safe_boards_changed() {
        let dir = tempdir().unwrap();
        let stored = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
            boards_dir: Some("old_hash".to_string()),
            shields_dir: None,
        };
        stored.save(dir.path()).unwrap();

        let current = BuildHashes {
            build_yaml: "abc".to_string(),
            west_yml: "def".to_string(),
            boards_dir: Some("new_hash".to_string()), // Changed!
            shields_dir: None,
        };
        assert!(!is_incremental_safe(dir.path(), &current));
    }
}
