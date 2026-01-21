use anyhow::Result;

use crate::config::project::Project;
use crate::container::Runtime;
use crate::output;
use crate::workspace::WorkspaceManager;

pub fn run() -> Result<()> {
    // 1. Detect project structure
    let project = Project::detect()?;
    output::status("Project", &project.root.display().to_string());

    // 2. Detect container runtime
    let runtime = Runtime::detect()?;
    output::status("Runtime", runtime.name());

    // 3. Get workspace manager
    let workspace_manager = WorkspaceManager::new()?;

    // 4. Force refresh the workspace
    let workspace = workspace_manager.refresh(&project, &runtime)?;
    output::status("Workspace", &workspace.display().to_string());

    Ok(())
}
