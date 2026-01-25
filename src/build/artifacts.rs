use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::target::BuildTarget;

/// Collect build artifacts from workspace to output directory
pub fn collect_artifact(
    workspace: &Path,
    target: &BuildTarget,
    output_dir: &Path,
) -> Result<PathBuf> {
    // Source path in workspace
    let source = workspace.join(target.firmware_path());

    // Ensure output directory exists
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    // Destination path
    let dest = output_dir.join(format!("{}.uf2", target.artifact_name));

    // Check if source exists
    if !source.exists() {
        anyhow::bail!("Build artifact not found at {}", source.display());
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
    fn test_collect_artifact() {
        let workspace = tempdir().unwrap();
        let output = tempdir().unwrap();

        // Create a fake build artifact
        let build_dir = workspace.path().join("build/test_target/zephyr");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("zmk.uf2"), "fake firmware").unwrap();

        let target = super::super::target::BuildTarget::from_args(
            "nice_nano_v2".to_string(),
            Some("test_target".to_string()),
        )
        .unwrap();

        // Manually fix the build_dir to match our test setup
        let mut target = target;
        target.build_dir = "build/test_target".to_string();
        target.artifact_name = "test_target".to_string();

        let result = collect_artifact(workspace.path(), &target, output.path());
        assert!(result.is_ok());

        let artifact_path = result.unwrap();
        assert!(artifact_path.exists());
        assert_eq!(artifact_path.file_name().unwrap(), "test_target.uf2");
    }
}
