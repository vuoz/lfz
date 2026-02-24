use anyhow::Result;

use crate::config::build_yaml::BuildInclude;

/// A resolved build target ready for building
#[derive(Debug, Clone)]
pub struct BuildTarget {
    /// Board identifier (e.g., "nice_nano_v2", "seeeduino_xiao_ble")
    pub board: String,

    /// Optional shield identifier (e.g., "corne_left", "cygnus_right")
    pub shield: Option<String>,

    /// CMake arguments to pass to west build
    pub cmake_args: Vec<String>,

    /// Zephyr snippets to apply
    pub snippet: Option<String>,

    /// Name for the output artifact (used for both build dir and output file)
    pub artifact_name: String,

    /// Build directory relative to workspace (e.g., "build/corne_left-nice_nano_v2")
    pub build_dir: String,

    /// Optional group for filtering (e.g., "central", "peripheral")
    pub group: Option<String>,
}

impl BuildTarget {
    /// Create a target from CLI arguments
    pub fn from_args(board: String, shield: Option<String>) -> Result<Self> {
        let artifact_name = Self::generate_artifact_name(&board, shield.as_deref());
        let build_dir = format!("build/{}", artifact_name);

        Ok(Self {
            board,
            shield,
            cmake_args: Vec::new(),
            snippet: None,
            artifact_name,
            build_dir,
            group: None,
        })
    }

    /// Create a target from a build.yaml include entry
    pub fn from_include(include: &BuildInclude) -> Result<Self> {
        let artifact_name = include.artifact_name.clone().unwrap_or_else(|| {
            Self::generate_artifact_name(&include.board, include.shield.as_deref())
        });

        let build_dir = format!("build/{}", artifact_name);

        // Parse cmake-args string into vec
        let cmake_args = include
            .cmake_args
            .as_ref()
            .map(|s| {
                s.split_whitespace()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(Self {
            board: include.board.clone(),
            shield: include.shield.clone(),
            cmake_args,
            snippet: include.snippet.clone(),
            artifact_name,
            build_dir,
            group: include.group.clone(),
        })
    }

    /// Sanitize a board identifier for use in filesystem paths.
    /// Replaces `//` (sysbuild domain qualifier) with `_` to avoid nested directories.
    /// e.g. "xiao_ble//zmk" -> "xiao_ble_zmk"
    fn sanitize_board(board: &str) -> String {
        board.replace("//", "_")
    }

    /// Generate artifact name from board and shield.
    /// Matches the ZMK GitHub Actions naming scheme:
    ///   ${artifact_name:-${shield:+$shield-}${board//\//_}-zmk}
    fn generate_artifact_name(board: &str, shield: Option<&str>) -> String {
        let sanitized = Self::sanitize_board(board);
        match shield {
            Some(s) => format!("{}-{}-zmk", s, sanitized),
            None => format!("{}-zmk", sanitized),
        }
    }

    /// Generate the west build command arguments
    pub fn west_build_args(&self, config_path: &str, pristine: bool) -> Vec<String> {
        let mut args = vec![
            "build".to_string(),
            "-s".to_string(),
            "zmk/app".to_string(), // Source directory
            "-d".to_string(),
            self.build_dir.clone(),
            "-b".to_string(),
            self.board.clone(),
        ];

        // Add pristine flag only if requested (clean rebuild)
        if pristine {
            args.push("-p".to_string());
        }

        // Add snippets if present (must be before -- separator)
        // Snippets can be space-separated, each needs its own -S flag
        if let Some(ref snippet) = self.snippet {
            for s in snippet.split_whitespace() {
                args.push("-S".to_string());
                args.push(s.to_string());
            }
        }

        // Add -- separator for CMake args
        args.push("--".to_string());

        // Always add ZMK_CONFIG
        args.push(format!("-DZMK_CONFIG={}", config_path));

        // Add shield if present
        if let Some(ref shield) = self.shield {
            args.push(format!("-DSHIELD={}", shield));
        }

        // Add any additional cmake args
        args.extend(self.cmake_args.clone());

        args
    }

    /// Get candidate paths for the output firmware file (relative to workspace root).
    /// Returns paths in priority order:
    ///   1. {build_dir}/zephyr/zmk.uf2  - standard or merged sysbuild output
    ///   2. {build_dir}/zmk/zephyr/zmk.uf2  - sysbuild zmk domain output
    pub fn firmware_path_candidates(&self) -> Vec<String> {
        vec![
            format!("{}/zephyr/zmk.uf2", self.build_dir),
            format!("{}/zmk/zephyr/zmk.uf2", self.build_dir),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_board_no_slashes() {
        assert_eq!(BuildTarget::sanitize_board("nice_nano_v2"), "nice_nano_v2");
    }

    #[test]
    fn test_sanitize_board_single_slash_preserved() {
        // Single / (SoC qualifier) is preserved as-is
        assert_eq!(
            BuildTarget::sanitize_board("xiao_ble/nrf52840"),
            "xiao_ble/nrf52840"
        );
    }

    #[test]
    fn test_sanitize_board_double_slash() {
        // // (sysbuild qualifier) is replaced with _
        assert_eq!(BuildTarget::sanitize_board("xiao_ble//zmk"), "xiao_ble_zmk");
    }

    #[test]
    fn test_from_args_with_shield() {
        let target =
            BuildTarget::from_args("nice_nano_v2".to_string(), Some("corne_left".to_string()))
                .unwrap();

        assert_eq!(target.board, "nice_nano_v2");
        assert_eq!(target.shield, Some("corne_left".to_string()));
        assert_eq!(target.artifact_name, "corne_left-nice_nano_v2-zmk");
        assert_eq!(target.build_dir, "build/corne_left-nice_nano_v2-zmk");
    }

    #[test]
    fn test_from_args_without_shield() {
        let target = BuildTarget::from_args("nice60".to_string(), None).unwrap();

        assert_eq!(target.board, "nice60");
        assert_eq!(target.shield, None);
        assert_eq!(target.artifact_name, "nice60-zmk");
    }

    #[test]
    fn test_from_args_hwmv2_board_with_shield() {
        let target =
            BuildTarget::from_args("xiao_ble//zmk".to_string(), Some("chalk_left".to_string()))
                .unwrap();

        assert_eq!(target.board, "xiao_ble//zmk"); // Original preserved for -b flag
        assert_eq!(target.artifact_name, "chalk_left-xiao_ble_zmk-zmk");
        assert_eq!(target.build_dir, "build/chalk_left-xiao_ble_zmk-zmk");
    }

    #[test]
    fn test_from_args_hwmv2_board_without_shield() {
        let target = BuildTarget::from_args("xiao_ble//zmk".to_string(), None).unwrap();

        assert_eq!(target.board, "xiao_ble//zmk");
        assert_eq!(target.artifact_name, "xiao_ble_zmk-zmk");
    }

    #[test]
    fn test_from_include_custom_artifact_name_preserved() {
        let include = BuildInclude {
            board: "xiao_ble//zmk".to_string(),
            shield: Some("chalk_left".to_string()),
            cmake_args: None,
            snippet: None,
            artifact_name: Some("my_custom_name".to_string()),
            group: None,
        };

        let target = BuildTarget::from_include(&include).unwrap();
        assert_eq!(target.artifact_name, "my_custom_name");
    }

    #[test]
    fn test_west_build_args_uses_original_board() {
        let target =
            BuildTarget::from_args("xiao_ble//zmk".to_string(), Some("chalk_left".to_string()))
                .unwrap();

        let args = target.west_build_args("/workspace/config", false);

        // -b flag must use the original board name (with //)
        assert!(args.contains(&"xiao_ble//zmk".to_string()));
        // build dir must be sanitized (no //)
        assert!(args.contains(&"build/chalk_left-xiao_ble_zmk-zmk".to_string()));
    }

    #[test]
    fn test_west_build_args_incremental() {
        let target =
            BuildTarget::from_args("nice_nano_v2".to_string(), Some("corne_left".to_string()))
                .unwrap();

        let args = target.west_build_args("/workspace/config", false);

        assert!(args.contains(&"build".to_string()));
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"zmk/app".to_string()));
        assert!(args.contains(&"-b".to_string()));
        assert!(args.contains(&"nice_nano_v2".to_string()));
        assert!(args.contains(&"-DSHIELD=corne_left".to_string()));
        assert!(args.iter().any(|a| a.contains("-DZMK_CONFIG=")));
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn test_west_build_args_pristine() {
        let target =
            BuildTarget::from_args("nice_nano_v2".to_string(), Some("corne_left".to_string()))
                .unwrap();

        let args = target.west_build_args("/workspace/config", true);

        assert!(args.contains(&"-p".to_string()));
    }

    #[test]
    fn test_west_build_args_with_snippet() {
        let include = BuildInclude {
            board: "seeeduino_xiao_ble".to_string(),
            shield: Some("cygnus_dongle".to_string()),
            cmake_args: None,
            snippet: Some("studio-rpc-usb-uart zmk-usb-logging".to_string()),
            artifact_name: None,
            group: None,
        };

        let target = BuildTarget::from_include(&include).unwrap();
        let args = target.west_build_args("/workspace/config", false);

        // Snippets should be -S flags before --
        let separator_pos = args.iter().position(|a| a == "--").unwrap();
        let s_positions: Vec<_> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "-S")
            .map(|(i, _)| i)
            .collect();

        for pos in &s_positions {
            assert!(
                *pos < separator_pos,
                "-S flag should be before -- separator"
            );
        }

        assert_eq!(s_positions.len(), 2);
        assert!(args.contains(&"studio-rpc-usb-uart".to_string()));
        assert!(args.contains(&"zmk-usb-logging".to_string()));
    }

    #[test]
    fn test_from_include_with_cmake_args() {
        let include = BuildInclude {
            board: "seeeduino_xiao_ble".to_string(),
            shield: Some("cygnus_left".to_string()),
            cmake_args: Some("-DCONFIG_ZMK_SPLIT=y -DCONFIG_ZMK_SPLIT_ROLE_CENTRAL=n".to_string()),
            snippet: None,
            artifact_name: None,
            group: None,
        };

        let target = BuildTarget::from_include(&include).unwrap();

        assert_eq!(target.cmake_args.len(), 2);
        assert!(target
            .cmake_args
            .contains(&"-DCONFIG_ZMK_SPLIT=y".to_string()));
    }

    #[test]
    fn test_firmware_path_candidates() {
        let target =
            BuildTarget::from_args("xiao_ble//zmk".to_string(), Some("chalk_left".to_string()))
                .unwrap();

        let candidates = target.firmware_path_candidates();
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0],
            "build/chalk_left-xiao_ble_zmk-zmk/zephyr/zmk.uf2"
        );
        assert_eq!(
            candidates[1],
            "build/chalk_left-xiao_ble_zmk-zmk/zmk/zephyr/zmk.uf2"
        );
    }
}
