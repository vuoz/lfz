use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::output;
use crate::paths;

/// Calculate directory size recursively
fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let mut size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                size += dir_size(&path);
            } else if let Ok(meta) = entry.metadata() {
                size += meta.len();
            }
        }
    }
    size
}

/// Format bytes as human-readable string
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Count items in directory
fn count_items(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    fs::read_dir(path).map(|e| e.count()).unwrap_or(0)
}

pub fn run() -> Result<()> {
    let cache_dir = paths::cache_dir()?;
    let workspaces_dir = paths::workspaces_dir()?;
    let ccache_dir = paths::ccache_dir()?;

    output::status("Cache", &paths::anonymize_path(&cache_dir));
    println!();

    // Workspaces
    let workspaces_size = dir_size(&workspaces_dir);
    let workspaces_count = count_items(&workspaces_dir);
    println!(
        "  Workspaces:  {:>10}  ({} workspace{})",
        format_size(workspaces_size),
        workspaces_count,
        if workspaces_count == 1 { "" } else { "s" }
    );

    // Ccache
    let ccache_size = dir_size(&ccache_dir);
    println!("  Ccache:      {:>10}", format_size(ccache_size));

    // Total
    let total_size = workspaces_size + ccache_size;
    println!("  ─────────────────────");
    println!("  Total:       {:>10}", format_size(total_size));

    Ok(())
}
