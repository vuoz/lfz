use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

use crate::build::orchestrator::BuildOrchestrator;
use crate::build::target::BuildTarget;
use crate::config::build_yaml::BuildConfig;
use crate::config::project::Project;
use crate::config::west_yml;
use crate::container::Runtime;
use crate::output;
use crate::paths;
use crate::workspace::WorkspaceManager;

#[allow(clippy::too_many_arguments)]
pub fn run(
    board: Option<String>,
    shield: Option<String>,
    output_path: String,
    jobs: Option<usize>,
    quiet: bool,
    verbose: bool,
    incremental: bool,
    group: String,
) -> Result<()> {
    // 1. Detect project structure
    let project = Project::detect()?;
    let project_display = west_yml::format_project_display(&project.config_dir)
        .unwrap_or_else(|_| paths::anonymize_path(&project.root));
    output::status("Project", &project_display);

    // 2. Detect container runtime and ensure it's running
    let runtime = Runtime::detect()?;
    output::status("Runtime", runtime.name());
    runtime.ensure_running()?;

    // 3. Get or create workspace
    let workspace_manager = WorkspaceManager::new()?;
    let workspace = workspace_manager.get_or_create(&project)?;
    output::status("Workspace", &paths::anonymize_path(&workspace));

    // 4. Determine build targets
    let targets = if let Some(board) = board {
        // Single target from CLI args (ignore group filter)
        vec![BuildTarget::from_args(board, shield)?]
    } else {
        // Parse build.yaml (path already detected by Project)
        let build_config = BuildConfig::load(&project.build_yaml)?;
        let all_targets = build_config.expand_targets()?;

        // Filter by group if specified (and not "all")
        if group == "all" {
            all_targets
        } else {
            let filtered: Vec<_> = all_targets
                .into_iter()
                .filter(|t| t.group.as_deref() == Some(group.as_str()))
                .collect();

            if filtered.is_empty() {
                anyhow::bail!(
                    "No targets found in group '{}'. Available groups: {}",
                    group,
                    build_config.available_groups().join(", ")
                );
            }
            filtered
        }
    };

    // Determine parallelism: -j1 = sequential, -jN = N parallel, default = all parallel
    let num_jobs = jobs.unwrap_or(targets.len()).max(1);

    if verbose {
        output::header(&format!(
            "Building {} target(s) with verbose output",
            targets.len()
        ));
    } else if num_jobs < targets.len() && num_jobs > 1 && targets.len() > 1 {
        output::header(&format!(
            "Building {} target(s) with {} parallel jobs",
            targets.len(),
            num_jobs
        ));
    } else {
        output::header(&format!("Building {} target(s)", targets.len()));
    }

    // 5. Run builds
    let output_dir = PathBuf::from(&output_path);
    // Pristine is the default (safe), incremental is opt-in (fast but may have stale artifacts)
    let pristine = !incremental;
    let orchestrator = BuildOrchestrator::new(
        runtime, workspace, project, output_dir, quiet, verbose, pristine,
    );

    let build_start = Instant::now();
    // Always use parallel build path (with progress bars) unless verbose mode
    // Verbose mode streams full output, so needs sequential handling
    let results = if verbose {
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
