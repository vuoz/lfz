use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

use crate::build::orchestrator::BuildOrchestrator;
use crate::build::target::BuildTarget;
use crate::config::build_yaml::BuildConfig;
use crate::config::project::Project;
use crate::container::Runtime;
use crate::output;
use crate::workspace::WorkspaceManager;

pub fn run(
    board: Option<String>,
    shield: Option<String>,
    output_path: String,
    jobs: Option<usize>,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    // 1. Detect project structure
    let project = Project::detect()?;
    output::status("Project", &project.root.display().to_string());

    // 2. Detect container runtime
    let runtime = Runtime::detect()?;
    output::status("Runtime", runtime.name());

    // 3. Get or create workspace
    let workspace_manager = WorkspaceManager::new()?;
    let workspace = workspace_manager.get_or_create(&project)?;
    output::status("Workspace", &workspace.display().to_string());

    // 4. Determine build targets
    let targets = if let Some(board) = board {
        // Single target from CLI args
        vec![BuildTarget::from_args(board, shield)?]
    } else {
        // Parse build.yaml (path already detected by Project)
        let build_config = BuildConfig::load(&project.build_yaml)?;
        build_config.expand_targets()?
    };

    // Determine parallelism: -j1 = sequential, -jN = N parallel, default = all parallel
    // Verbose mode forces sequential builds for readable output
    let num_jobs = if verbose {
        1 // Verbose mode requires sequential for readable streaming output
    } else {
        jobs.unwrap_or(targets.len()).max(1)
    };

    if verbose {
        output::header(&format!(
            "Building {} target(s) with verbose output",
            targets.len()
        ));
    } else if num_jobs == 1 {
        output::header(&format!(
            "Building {} target(s) sequentially",
            targets.len()
        ));
    } else if num_jobs >= targets.len() {
        output::header(&format!("Building {} target(s)", targets.len()));
    } else {
        output::header(&format!(
            "Building {} target(s) with {} parallel jobs",
            targets.len(),
            num_jobs
        ));
    }

    // 5. Run builds
    let output_dir = PathBuf::from(&output_path);
    let orchestrator =
        BuildOrchestrator::new(runtime, workspace, project, output_dir, quiet, verbose);

    let build_start = Instant::now();
    let results = if num_jobs == 1 {
        orchestrator.build_sequential(&targets)?
    } else {
        orchestrator.build_parallel(&targets, num_jobs)?
    };
    let total_time = build_start.elapsed();

    // 6. Report results
    let succeeded: Vec<_> = results.iter().filter(|r| r.success).collect();
    let failed: Vec<_> = results.iter().filter(|r| !r.success).collect();

    output::summary(succeeded.len(), failed.len(), Some(total_time));

    if !failed.is_empty() {
        output::header("Failed builds");
        for result in &failed {
            output::error(&format!(
                "{}: {}",
                result.target_name,
                result.error.as_deref().unwrap_or("unknown error")
            ));

            // Show the build error output if available
            if let Some(error_output) = &result.error_output {
                // Print a separator and the error output
                println!();
                output::build_error_output(&result.target_name, error_output);
            }
        }
        anyhow::bail!("{} build(s) failed", failed.len());
    }

    output::header(&format!("Firmware written to {}", output_path));
    for result in &succeeded {
        if let Some(artifact) = &result.artifact_path {
            output::list_item(&artifact.display().to_string());
        }
    }

    Ok(())
}
