use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::target::BuildTarget;

/// Collect build artifacts from workspace to output directory.
/// Searches multiple candidate paths to support both standard and sysbuild layouts,
/// and both .uf2 and .hex firmware formats.
pub fn collect_artifact(
    workspace: &Path,
    target: &BuildTarget,
    output_dir: &Path,
) -> Result<PathBuf> {
    // Find the first existing firmware file from the candidate paths
    let candidates = target.firmware_path_candidates();
    let source = candidates
        .iter()
        .map(|c| workspace.join(c))
        .find(|p| p.exists())
        .with_context(|| {
            let tried: Vec<String> = candidates
                .iter()
                .map(|c| workspace.join(c).display().to_string())
                .collect();
            format!(
                "Build artifact not found. Searched:\n  {}",
                tried.join("\n  ")
            )
        })?;

    // Destination path
    let dest = output_dir.join(format!("{}.uf2", target.artifact_name));

    // Ensure all parent directories of the destination exist
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
    }

    // Copy the artifact
    fs::copy(&source, &dest)
        .with_context(|| format!("Failed to copy {} to {}", source.display(), dest.display()))?;

    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_collect_artifact_uf2() {
        let workspace = tempdir().unwrap();
        let output = tempdir().unwrap();

        // Create a fake build artifact at the standard path
        let build_dir = workspace.path().join("build/test_target-zmk/zephyr");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("zmk.uf2"), "fake firmware").unwrap();

        let mut target = super::super::target::BuildTarget::from_args(
            "nice_nano_v2".to_string(),
            Some("test_target".to_string()),
        )
        .unwrap();
        target.build_dir = "build/test_target-zmk".to_string();
        target.artifact_name = "test_target-zmk".to_string();

        let result = collect_artifact(workspace.path(), &target, output.path());
        assert!(result.is_ok());

        let artifact_path = result.unwrap();
        assert!(artifact_path.exists());
        assert_eq!(artifact_path.file_name().unwrap(), "test_target-zmk.uf2");
    }

    #[test]
    fn test_collect_artifact_sysbuild_fallback() {
        let workspace = tempdir().unwrap();
        let output = tempdir().unwrap();

        // Only create firmware at the sysbuild zmk domain path (no top-level zephyr/)
        let build_dir = workspace.path().join("build/test_target-zmk/zmk/zephyr");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("zmk.uf2"), "fake sysbuild firmware").unwrap();

        let mut target = super::super::target::BuildTarget::from_args(
            "nice_nano_v2".to_string(),
            Some("test_target".to_string()),
        )
        .unwrap();
        target.build_dir = "build/test_target-zmk".to_string();
        target.artifact_name = "test_target-zmk".to_string();

        let result = collect_artifact(workspace.path(), &target, output.path());
        assert!(result.is_ok());

        let artifact_path = result.unwrap();
        assert!(artifact_path.exists());
        assert_eq!(artifact_path.file_name().unwrap(), "test_target-zmk.uf2");
    }

    #[test]
    fn test_collect_artifact_not_found() {
        let workspace = tempdir().unwrap();
        let output = tempdir().unwrap();

        // Don't create any firmware files
        let build_dir = workspace.path().join("build/test_target-zmk/zephyr");
        fs::create_dir_all(&build_dir).unwrap();

        let mut target = super::super::target::BuildTarget::from_args(
            "nice_nano_v2".to_string(),
            Some("test_target".to_string()),
        )
        .unwrap();
        target.build_dir = "build/test_target-zmk".to_string();
        target.artifact_name = "test_target-zmk".to_string();

        let result = collect_artifact(workspace.path(), &target, output.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Build artifact not found"));
    }
}
