use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::build::target::BuildTarget;

/// Represents a build.yaml file that defines build targets
#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    /// Top-level list of boards to build for all shields
    #[serde(default)]
    pub board: Vec<String>,

    /// Top-level list of shields to build for all boards
    #[serde(default)]
    pub shield: Vec<String>,

    /// Specific board+shield combinations with additional options
    #[serde(default)]
    pub include: Vec<BuildInclude>,
}

/// A specific build configuration from the include array
#[derive(Debug, Deserialize, Clone)]
pub struct BuildInclude {
    pub board: String,

    #[serde(default)]
    pub shield: Option<String>,

    #[serde(rename = "cmake-args")]
    pub cmake_args: Option<String>,

    #[serde(default)]
    pub snippet: Option<String>,

    #[serde(rename = "artifact-name")]
    pub artifact_name: Option<String>,

    /// Optional group for filtering (e.g., "central", "peripheral")
    #[serde(default)]
    pub group: Option<String>,
}

impl BuildConfig {
    /// Load build.yaml from a path
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read build.yaml at {}", path.display()))?;

        serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse build.yaml at {}", path.display()))
    }

    /// Expand the build config into a list of concrete build targets
    pub fn expand_targets(&self) -> Result<Vec<BuildTarget>> {
        let mut targets = Vec::new();

        // First, handle explicit includes
        for include in &self.include {
            targets.push(BuildTarget::from_include(include)?);
        }

        // Then, if board and shield arrays are specified, create cartesian product
        // but only if include is empty (to avoid duplicates)
        if self.include.is_empty() && !self.board.is_empty() {
            if self.shield.is_empty() {
                // Just boards, no shields
                for board in &self.board {
                    targets.push(BuildTarget::from_args(board.clone(), None)?);
                }
            } else {
                // Cartesian product of boards × shields
                for board in &self.board {
                    for shield in &self.shield {
                        targets.push(BuildTarget::from_args(board.clone(), Some(shield.clone()))?);
                    }
                }
            }
        }

        if targets.is_empty() {
            anyhow::bail!("No build targets found in build.yaml");
        }

        Ok(targets)
    }

    /// Get list of unique groups defined in the config
    pub fn available_groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = self
            .include
            .iter()
            .filter_map(|inc| inc.group.clone())
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_build_yaml() {
        let yaml = r#"
board:
  - nice_nano_v2
shield:
  - corne_left
  - corne_right
"#;
        let config: BuildConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.board, vec!["nice_nano_v2"]);
        assert_eq!(config.shield, vec!["corne_left", "corne_right"]);
    }

    #[test]
    fn test_parse_include_build_yaml() {
        let yaml = r#"
include:
  - board: seeeduino_xiao_ble
    shield: cygnus_left
    cmake-args: -DCONFIG_ZMK_SPLIT=y
  - board: seeeduino_xiao_ble
    shield: cygnus_right
    artifact-name: cygnus_right_custom
"#;
        let config: BuildConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.include[0].board, "seeeduino_xiao_ble");
        assert_eq!(config.include[0].shield, Some("cygnus_left".to_string()));
        assert_eq!(
            config.include[0].cmake_args,
            Some("-DCONFIG_ZMK_SPLIT=y".to_string())
        );
        assert_eq!(
            config.include[1].artifact_name,
            Some("cygnus_right_custom".to_string())
        );
    }

    #[test]
    fn test_expand_cartesian_product() {
        let yaml = r#"
board:
  - nice_nano_v2
shield:
  - corne_left
  - corne_right
"#;
        let config: BuildConfig = serde_yaml::from_str(yaml).unwrap();
        let targets = config.expand_targets().unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].artifact_name, "corne_left-nice_nano_v2");
        assert_eq!(targets[1].artifact_name, "corne_right-nice_nano_v2");
    }

    #[test]
    fn test_parse_include_with_group() {
        let yaml = r#"
include:
  - board: nice_nano_v2
    shield: corne_left
    group: central
  - board: nice_nano_v2
    shield: corne_right
    group: peripheral
"#;
        let config: BuildConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.include[0].group, Some("central".to_string()));
        assert_eq!(config.include[1].group, Some("peripheral".to_string()));

        let targets = config.expand_targets().unwrap();
        assert_eq!(targets[0].group, Some("central".to_string()));
        assert_eq!(targets[1].group, Some("peripheral".to_string()));
    }

    #[test]
    fn test_available_groups() {
        let yaml = r#"
include:
  - board: nice_nano_v2
    shield: corne_left
    group: central
  - board: nice_nano_v2
    shield: corne_right
    group: peripheral
  - board: nice_nano_v2
    shield: corne_dongle
    group: central
"#;
        let config: BuildConfig = serde_yaml::from_str(yaml).unwrap();
        let groups = config.available_groups();
        assert_eq!(groups, vec!["central", "peripheral"]);
    }
}
