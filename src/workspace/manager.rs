use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;

use crate::config::project::Project;
use crate::config::west_yml;
use crate::container::{ContainerCommand, Runtime, DEFAULT_IMAGE};
use crate::output;
use crate::paths;

/// Manages west workspaces for building ZMK
pub struct WorkspaceManager {
    /// Root directory for all cached workspaces
    workspaces_dir: PathBuf,
    /// Shared ccache directory
    ccache_dir: PathBuf,
}

impl WorkspaceManager {
    pub fn new() -> Result<Self> {
        let workspaces_dir = paths::workspaces_dir()?;
        let ccache_dir = paths::ccache_dir()?;

        // Ensure directories exist
        fs::create_dir_all(&workspaces_dir).context("Failed to create workspaces directory")?;
        fs::create_dir_all(&ccache_dir).context("Failed to create ccache directory")?;

        Ok(Self {
            workspaces_dir,
            ccache_dir,
        })
    }

    /// Get the workspace path for a project (based on west.yml hash)
    pub fn workspace_path(&self, project: &Project) -> Result<PathBuf> {
        let hash = west_yml::hash_west_yml(&project.west_yml)?;
        Ok(self.workspaces_dir.join(hash))
    }

    /// Find existing workspace for a project, if any
    pub fn find_workspace(&self, project: &Project) -> Result<Option<PathBuf>> {
        let workspace = self.workspace_path(project)?;
        if workspace.exists() && workspace.join(".west").exists() {
            Ok(Some(workspace))
        } else {
            Ok(None)
        }
    }

    /// Get or create a workspace for a project
    pub fn get_or_create(&self, project: &Project) -> Result<PathBuf> {
        let workspace = self.workspace_path(project)?;

        // Check if workspace already exists and is initialized
        if workspace.join(".west").exists() {
            output::info("Using cached workspace");
            return Ok(workspace);
        }

        // Need to initialize workspace
        output::header("Initializing new workspace");
        self.initialize_workspace(&workspace, project)?;

        Ok(workspace)
    }

    /// Force refresh the workspace (re-run west update)
    pub fn refresh(&self, project: &Project, runtime: &Runtime) -> Result<PathBuf> {
        let workspace = self.workspace_path(project)?;

        // Remove existing workspace if present
        if workspace.exists() {
            output::info("Removing existing workspace...");
            fs::remove_dir_all(&workspace).context("Failed to remove existing workspace")?;
        }

        // Re-initialize
        output::header("Reinitializing workspace");

        // We need a runtime to initialize
        self.initialize_workspace_with_runtime(&workspace, project, runtime)?;

        Ok(workspace)
    }

    /// Initialize a new workspace
    fn initialize_workspace(&self, workspace: &PathBuf, project: &Project) -> Result<()> {
        // Detect runtime for initialization
        let runtime = Runtime::detect()?;
        self.initialize_workspace_with_runtime(workspace, project, &runtime)
    }

    /// Initialize a new workspace with a specific runtime
    fn initialize_workspace_with_runtime(
        &self,
        workspace: &PathBuf,
        project: &Project,
        runtime: &Runtime,
    ) -> Result<()> {
        // Create workspace directory
        fs::create_dir_all(workspace).context("Failed to create workspace directory")?;

        // Ensure image is available
        runtime.ensure_image(DEFAULT_IMAGE)?;

        // Build the west init && west update command
        // We mount the config as read-only and let west clone everything into the workspace
        // Retry west update up to 3 times since network failures are common
        let init_script = r#"
set -e
echo "Initializing west workspace..."
west init -l /workspace/config

echo "Updating west modules (this may take a while)..."
max_retries=3
retry_count=0
until west update; do
    retry_count=$((retry_count + 1))
    if [ $retry_count -ge $max_retries ]; then
        echo "ERROR: west update failed after $max_retries attempts"
        exit 1
    fi
    echo "west update failed, retrying ($retry_count/$max_retries)..."
    sleep 2
done

echo "Workspace initialized successfully"
"#;

        let mut cmd = ContainerCommand::new(runtime.clone(), DEFAULT_IMAGE)
            .mount(workspace, "/workspace", false)
            .mount(&project.config_dir, "/workspace/config", true)
            .mount(&self.ccache_dir, "/root/.ccache", false)
            .workdir("/workspace")
            .shell_command(init_script)
            .build();

        output::command("west init -l config && west update");
        output::info("This may take several minutes on first run...");

        // Stream output so user can see progress
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context("Failed to run container for workspace initialization")?;

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        // Stream stdout in a separate thread
        let stdout_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut last_lines: Vec<String> = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                // Show progress lines (cloning, fetching, etc.)
                if line.contains("Cloning")
                    || line.contains("Fetching")
                    || line.contains("Updating")
                    || line.contains("=== ")
                    || line.contains("initialized")
                    || line.contains("ERROR")
                    || line.contains("error:")
                {
                    println!("  {}", line);
                }
                // Keep last lines for error context
                last_lines.push(line);
                if last_lines.len() > 30 {
                    last_lines.remove(0);
                }
            }
            last_lines
        });

        // Stream stderr
        let stderr_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut error_output = String::new();
            for line in reader.lines().map_while(Result::ok) {
                eprintln!("  {}", line);
                error_output.push_str(&line);
                error_output.push('\n');
            }
            error_output
        });

        let status = child
            .wait()
            .context("Failed to wait for workspace initialization")?;
        let last_lines = stdout_handle.join().unwrap_or_default();
        let stderr_output = stderr_handle.join().unwrap_or_default();

        if !status.success() {
            // Show last stdout lines for context
            if !last_lines.is_empty() {
                eprintln!("\nLast output:");
                for line in &last_lines[last_lines.len().saturating_sub(15)..] {
                    eprintln!("  {}", line);
                }
            }
            if !stderr_output.is_empty() && !stderr_output.trim().is_empty() {
                eprintln!("\nErrors:\n{}", stderr_output);
            }

            // Clean up failed workspace
            let _ = fs::remove_dir_all(workspace);
            output::error("Workspace initialization failed");
            output::info(
                "Tip: This is often a transient network error. Try running 'lfz build' again.",
            );
            anyhow::bail!("Workspace initialization failed");
        }

        output::success("Workspace initialized successfully");

        Ok(())
    }

    /// Get the ccache directory path
    pub fn ccache_dir(&self) -> &PathBuf {
        &self.ccache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_manager_new() {
        // This should succeed even without an actual project
        let manager = WorkspaceManager::new();
        assert!(manager.is_ok());
    }
}
