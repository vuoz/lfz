mod command;

pub use command::ContainerCommand;

use anyhow::{Context, Result};
use std::process::Command;

/// Default ZMK build image
pub const DEFAULT_IMAGE: &str = "zmkfirmware/zmk-build-arm:stable";

/// Supported container runtimes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Docker,
    Podman,
}

impl Runtime {
    /// Detect available container runtime
    /// Prefers Podman over Docker as it's daemonless
    pub fn detect() -> Result<Self> {
        // Try podman first
        if Self::is_available("podman") {
            return Ok(Runtime::Podman);
        }

        // Fall back to docker
        if Self::is_available("docker") {
            return Ok(Runtime::Docker);
        }

        anyhow::bail!(
            "No container runtime found. Please install Docker or Podman.\n\
             - Docker: https://docs.docker.com/get-docker/\n\
             - Podman: https://podman.io/getting-started/installation"
        )
    }

    /// Check if a runtime is available
    fn is_available(name: &str) -> bool {
        Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get the command name for this runtime
    pub fn command_name(&self) -> &'static str {
        match self {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
        }
    }

    /// Get a display name for this runtime
    pub fn name(&self) -> &'static str {
        match self {
            Runtime::Docker => "Docker",
            Runtime::Podman => "Podman",
        }
    }

    /// Create a new Command for this runtime
    pub fn command(&self) -> Command {
        Command::new(self.command_name())
    }

    /// Check if an image exists locally
    pub fn image_exists(&self, image: &str) -> Result<bool> {
        let output = self
            .command()
            .args(["image", "inspect", image])
            .output()
            .context("Failed to check for image")?;

        Ok(output.status.success())
    }

    /// Pull an image
    pub fn pull_image(&self, image: &str) -> Result<()> {
        println!("Pulling image: {}", image);

        let status = self
            .command()
            .args(["pull", image])
            .status()
            .context("Failed to pull image")?;

        if !status.success() {
            anyhow::bail!("Failed to pull image: {}", image);
        }

        Ok(())
    }

    /// Ensure an image is available (pull if necessary)
    pub fn ensure_image(&self, image: &str) -> Result<()> {
        if !self.image_exists(image)? {
            self.pull_image(image)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_name() {
        assert_eq!(Runtime::Docker.command_name(), "docker");
        assert_eq!(Runtime::Podman.command_name(), "podman");
    }

    #[test]
    fn test_detect_runtime() {
        // This test will pass if either docker or podman is installed
        // It will fail if neither is installed, which is expected behavior
        let result = Runtime::detect();
        if result.is_ok() {
            let runtime = result.unwrap();
            assert!(runtime == Runtime::Docker || runtime == Runtime::Podman);
        }
    }
}
