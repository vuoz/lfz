use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::config::project::Project;
use crate::output;
use crate::paths;
use crate::workspace::WorkspaceManager;

/// Recursively remove a directory, fixing permissions as needed.
/// Some files (like git objects) may be read-only.
pub fn remove_dir_all(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // First try normal removal
    if fs::remove_dir_all(path).is_ok() {
        return Ok(());
    }

    // If that failed, fix permissions and try again
    fix_permissions(path)?;
    fs::remove_dir_all(path).with_context(|| format!("Failed to remove {}", path.display()))
}

/// Recursively make all files and directories writable
fn fix_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.is_dir() {
        // Make directory writable first so we can modify contents
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o700);
        fs::set_permissions(path, perms)?;

        // Then recurse into contents
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            fix_permissions(&entry.path())?;
        }
    } else {
        // Make file writable
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o600);
        fs::set_permissions(path, perms)?;
    }

    Ok(())
}

pub fn run(all: bool) -> Result<()> {
    if all {
        // Remove all cached workspaces
        let workspaces_dir = paths::workspaces_dir()?;
        if workspaces_dir.exists() {
            let spinner = output::spinner(&format!(
                "Removing all cached workspaces: {}",
                workspaces_dir.display()
            ));
            remove_dir_all(&workspaces_dir)?;
            spinner.finish_with_message("All cached workspaces removed.");
        } else {
            output::info("No cached workspaces found.");
        }
    } else {
        // Remove workspace for current project
        let project = Project::detect()?;
        let workspace_manager = WorkspaceManager::new()?;

        if let Some(workspace) = workspace_manager.find_workspace(&project)? {
            let spinner = output::spinner(&format!("Removing workspace: {}", workspace.display()));
            remove_dir_all(&workspace)?;
            spinner.finish_with_message("Workspace removed.");
        } else {
            output::info("No cached workspace found for this project.");
        }
    }

    Ok(())
}
