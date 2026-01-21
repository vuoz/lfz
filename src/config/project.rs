use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

/// Represents a detected ZMK keyboard project
#[derive(Debug)]
pub struct Project {
    /// Root directory of the project (where lfz is invoked)
    pub root: PathBuf,

    /// Path to the config directory (contains west.yml, keymaps, etc.)
    pub config_dir: PathBuf,

    /// Optional path to boards directory at root level
    pub boards_dir: Option<PathBuf>,

    /// Path to build.yaml or build.yml (in root directory)
    pub build_yaml: PathBuf,

    /// Path to west.yml (in config directory)
    pub west_yml: PathBuf,

    /// Whether the project root is a valid Zephyr module (has zephyr/module.yml)
    pub is_zephyr_module: bool,
}

impl Project {
    /// Detect project structure from current working directory
    pub fn detect() -> Result<Self> {
        let cwd = env::current_dir().context("Failed to get current directory")?;
        Self::detect_from(&cwd)
    }

    /// Detect project structure from a given directory
    pub fn detect_from(root: &PathBuf) -> Result<Self> {
        let config_dir = root.join("config");

        // Verify config directory exists
        if !config_dir.is_dir() {
            anyhow::bail!(
                "No 'config' directory found in {}. \
                 Please run lfz from the root of your ZMK config repository.",
                root.display()
            );
        }

        // Check for build.yaml or build.yml in root directory
        let build_yaml = root.join("build.yaml");
        let build_yml = root.join("build.yml");
        let build_yaml = if build_yaml.is_file() {
            build_yaml
        } else if build_yml.is_file() {
            build_yml
        } else {
            anyhow::bail!(
                "No 'build.yaml' or 'build.yml' found in {}. \
                 This file is required to define build targets.",
                root.display()
            );
        };

        // Check for west.yml
        let west_yml = config_dir.join("west.yml");
        if !west_yml.is_file() {
            anyhow::bail!(
                "No 'west.yml' found in {}. \
                 This file is required to define ZMK and module dependencies.",
                config_dir.display()
            );
        }

        // Check for optional boards directory at root level
        let boards_dir = root.join("boards");
        let boards_dir = if boards_dir.is_dir() {
            Some(boards_dir)
        } else {
            None
        };

        // Check if project root is a valid Zephyr module (has zephyr/module.yml)
        let is_zephyr_module = root.join("zephyr").join("module.yml").is_file();

        Ok(Self {
            root: root.clone(),
            config_dir,
            boards_dir,
            build_yaml,
            west_yml,
            is_zephyr_module,
        })
    }

    /// Check if there's a boards directory (either at root or in config)
    pub fn has_custom_boards(&self) -> bool {
        self.boards_dir.is_some() || self.config_dir.join("boards").is_dir()
    }

    /// Get Zephyr extra modules that need to be mounted
    ///
    /// If the project root has zephyr/module.yml, mount the entire root as a module.
    /// This is the standard ZMK config structure where boards/ is inside a Zephyr module.
    pub fn extra_modules(&self) -> Vec<PathBuf> {
        // If project root is a Zephyr module, use it
        if self.is_zephyr_module {
            return vec![self.root.clone()];
        }

        // Otherwise, no extra modules
        // (boards/ alone without zephyr/module.yml is not a valid module)
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_valid_project() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let config_dir = root.join("config");
        fs::create_dir(&config_dir).unwrap();
        // build.yaml is in root, west.yml is in config
        fs::write(root.join("build.yaml"), "board: [nice_nano_v2]").unwrap();
        fs::write(config_dir.join("west.yml"), "manifest:\n  projects: []").unwrap();

        let project = Project::detect_from(&root.to_path_buf()).unwrap();
        assert_eq!(project.config_dir, config_dir);
        assert_eq!(project.build_yaml, root.join("build.yaml"));
        assert!(project.boards_dir.is_none());
    }

    #[test]
    fn test_detect_with_build_yml() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let config_dir = root.join("config");
        fs::create_dir(&config_dir).unwrap();
        // Use build.yml instead of build.yaml
        fs::write(root.join("build.yml"), "board: [nice_nano_v2]").unwrap();
        fs::write(config_dir.join("west.yml"), "manifest:\n  projects: []").unwrap();

        let project = Project::detect_from(&root.to_path_buf()).unwrap();
        assert_eq!(project.build_yaml, root.join("build.yml"));
    }

    #[test]
    fn test_detect_with_boards() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let config_dir = root.join("config");
        let boards_dir = root.join("boards");
        fs::create_dir(&config_dir).unwrap();
        fs::create_dir(&boards_dir).unwrap();
        fs::write(root.join("build.yaml"), "board: [nice_nano_v2]").unwrap();
        fs::write(config_dir.join("west.yml"), "manifest:\n  projects: []").unwrap();

        let project = Project::detect_from(&root.to_path_buf()).unwrap();
        assert_eq!(project.boards_dir, Some(boards_dir));
        // boards/ alone is not a Zephyr module
        assert!(!project.is_zephyr_module);
        assert!(project.extra_modules().is_empty());
    }

    #[test]
    fn test_detect_zephyr_module() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let config_dir = root.join("config");
        let zephyr_dir = root.join("zephyr");
        fs::create_dir(&config_dir).unwrap();
        fs::create_dir(&zephyr_dir).unwrap();
        fs::write(root.join("build.yaml"), "board: [nice_nano_v2]").unwrap();
        fs::write(config_dir.join("west.yml"), "manifest:\n  projects: []").unwrap();
        fs::write(zephyr_dir.join("module.yml"), "build:\n  cmake: zephyr").unwrap();

        let project = Project::detect_from(&root.to_path_buf()).unwrap();
        assert!(project.is_zephyr_module);
        assert_eq!(project.extra_modules(), vec![root.to_path_buf()]);
    }
}
