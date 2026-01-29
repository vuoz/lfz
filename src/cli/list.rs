use anyhow::Result;

use crate::config::build_yaml::BuildConfig;
use crate::config::project::Project;
use crate::output;

/// Run the list command - show available build targets and groups
pub fn run(group: Option<String>) -> Result<()> {
    let project = Project::detect()?;
    let build_config = BuildConfig::load(&project.build_yaml)?;
    let targets = build_config.expand_targets()?;
    let groups = build_config.available_groups();

    // Filter by group if specified
    let filtered_targets: Vec<_> = if let Some(ref g) = group {
        targets
            .into_iter()
            .filter(|t| t.group.as_deref() == Some(g.as_str()))
            .collect()
    } else {
        targets
    };

    // Show groups if any exist
    if !groups.is_empty() {
        output::header("Groups");
        for g in &groups {
            output::list_item(g);
        }
    }

    // Show targets
    let header = if let Some(ref g) = group {
        format!("Targets in group '{}'", g)
    } else {
        format!("Targets ({})", filtered_targets.len())
    };
    output::header(&header);

    if filtered_targets.is_empty() {
        if let Some(g) = group {
            output::error(&format!("No targets found in group '{}'", g));
            if !groups.is_empty() {
                output::info(&format!("Available groups: {}", groups.join(", ")));
            }
        } else {
            output::error("No targets found in build.yaml");
        }
        return Ok(());
    }

    for target in &filtered_targets {
        let group_suffix = target
            .group
            .as_ref()
            .map(|g| format!(" [{}]", g))
            .unwrap_or_default();

        let details = format!(
            "board: {}{}",
            target.board,
            target
                .shield
                .as_ref()
                .map(|s| format!(", shield: {}", s))
                .unwrap_or_default()
        );

        println!(
            "  {} {}{}",
            console::style(&target.artifact_name).cyan(),
            console::style(details).dim(),
            console::style(group_suffix).yellow()
        );
    }

    Ok(())
}
