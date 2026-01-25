//! Terminal output utilities using indicatif and console

use console::{style, Style, Term};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Create a spinner for long-running operations
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Build progress tracker for parallel builds using indicatif MultiProgress
pub struct BuildProgress {
    multi: MultiProgress,
    bars: Vec<ProgressBar>,
    targets: Vec<String>,
    results: Mutex<Vec<(bool, String)>>, // (success, message) for each target
}

impl BuildProgress {
    /// Create a new build progress tracker for the given targets
    pub fn new(targets: &[String]) -> Self {
        let multi = MultiProgress::new();

        // Use stderr for progress so it doesn't interfere with piped output
        multi.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));

        let mut bars = Vec::new();

        let pb_style = ProgressStyle::default_spinner()
            .template("{prefix} {msg}")
            .unwrap();

        for target in targets {
            let pb = multi.add(ProgressBar::new_spinner());
            pb.set_style(pb_style.clone());
            pb.set_prefix(format!("{}", style("[  ]").dim()));
            pb.set_message(format!("{} waiting", target));
            pb.enable_steady_tick(Duration::from_millis(100));
            bars.push(pb);
        }

        let results = Mutex::new(vec![(false, String::new()); targets.len()]);

        Self {
            multi,
            bars,
            targets: targets.to_vec(),
            results,
        }
    }

    /// Update a target's status
    pub fn update(&self, index: usize, state: BuildState, message: &str) {
        if let Some(pb) = self.bars.get(index) {
            let target = self.targets.get(index).map(|s| s.as_str()).unwrap_or("");
            let prefix = match state {
                BuildState::Pending => format!("{}", style("[  ]").dim()),
                BuildState::Starting => format!("{}", style("[..]").cyan()),
                BuildState::Running => format!("{}", style("[>>]").blue()),
                BuildState::Success => format!("{}", style("[OK]").green().bold()),
                BuildState::Failed => format!("{}", style("[XX]").red().bold()),
            };

            pb.set_prefix(prefix);
            if message.is_empty() {
                pb.set_message(target.to_string());
            } else {
                pb.set_message(format!("{} {}", target, style(message).dim()));
            }
        }
    }

    /// Mark a target as complete with result
    pub fn finish(
        &self,
        index: usize,
        success: bool,
        artifact: Option<&str>,
        duration: Option<Duration>,
    ) {
        if let Some(pb) = self.bars.get(index) {
            let target = self.targets.get(index).map(|s| s.as_str()).unwrap_or("");
            let time_str = duration
                .map(|d| format!("({})", format_duration(d)))
                .unwrap_or_default();

            let msg = if success {
                if let Some(art) = artifact {
                    format!("{} {} {}", target, art, time_str)
                } else {
                    format!("{} {}", target, time_str)
                }
            } else {
                format!("{} failed {}", target, time_str)
            };

            // Store result for final printing
            if let Ok(mut results) = self.results.lock() {
                if index < results.len() {
                    results[index] = (success, msg.clone());
                }
            }

            // Update progress bar
            let prefix = if success {
                format!("{}", style("[OK]").green().bold())
            } else {
                format!("{}", style("[XX]").red().bold())
            };
            pb.set_prefix(prefix);
            pb.finish_with_message(msg);
        }
    }

    /// Print final results to stdout (call after all builds complete)
    pub fn print_results(&self) {
        // First, finish and clear all progress bars
        for pb in &self.bars {
            pb.finish_and_clear();
        }

        // Print results to stdout
        if let Ok(results) = self.results.lock() {
            for (success, msg) in results.iter() {
                if msg.is_empty() {
                    continue;
                }
                if *success {
                    println!("{} {}", style("[OK]").green().bold(), msg);
                } else {
                    println!("{} {}", style("[XX]").red().bold(), msg);
                }
            }
        }
    }

    /// Finish all progress bars (call when done)
    pub fn finish_all(&self) {
        for pb in &self.bars {
            if !pb.is_finished() {
                pb.finish();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildState {
    Pending,
    Starting,
    Running,
    Success,
    Failed,
}

// === Simple output functions using console ===

/// Print a status message (cyan, bold prefix)
pub fn status(prefix: &str, message: &str) {
    println!("{} {}", style(prefix).cyan().bold(), message);
}

/// Print an info message (blue)
pub fn info(message: &str) {
    println!("{}", style(message).blue());
}

/// Print a success message (green)
pub fn success(message: &str) {
    println!("{}", style(message).green());
}

/// Print a warning message (yellow)
pub fn warning(message: &str) {
    println!("{} {}", style("warning:").yellow(), message);
}

/// Print an error message (red)
pub fn error(message: &str) {
    eprintln!("{} {}", style("error:").red(), message);
}

/// Print a section header
pub fn header(message: &str) {
    println!("\n{}", style(format!("==> {}", message)).magenta().bold());
}

/// Print a list item
pub fn list_item(item: &str) {
    println!("  {} {}", style("-").dim(), item);
}

/// Print a key-value pair
pub fn kv(key: &str, value: &str) {
    println!("  {} {}", style(format!("{}:", key)).dim(), value);
}

/// Print command being executed (dimmed)
pub fn command(cmd: &str) {
    println!("{}", style(format!("$ {}", cmd)).dim());
}

/// Format a duration as human-readable string
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 60 {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// Print the final summary with optional timing
pub fn summary(succeeded: usize, failed: usize, total_time: Option<Duration>) {
    println!();
    let time_str = total_time
        .map(|d| format!(" in {}", format_duration(d)))
        .unwrap_or_default();

    if failed == 0 {
        println!(
            "{} {} succeeded, {} failed{}",
            style("Build complete:").green().bold(),
            succeeded,
            failed,
            time_str
        );
    } else {
        println!(
            "{} {} succeeded, {}{}",
            style("Build complete:").red().bold(),
            style(format!("{}", succeeded)).green(),
            style(format!("{} failed", failed)).red(),
            time_str
        );
    }
}

/// Print build error output with formatting
pub fn build_error_output(target: &str, output: &str) {
    println!("{}", style(format!("--- Output for {} ---", target)).dim());

    for line in output.lines() {
        if line.contains("error:") || line.contains("Error") || line.contains("FATAL") {
            println!("{}", style(line).red());
        } else if line.contains("warning:") {
            println!("{}", style(line).yellow());
        } else {
            println!("{}", style(line).dim());
        }
    }

    println!("{}", style("--- End output ---").dim());
}

// === Verbose output functions ===

/// Print a header for verbose build output (sequential mode)
pub fn verbose_header(target: &str) {
    let separator = "=".repeat(60);
    println!("\n{}", style(&separator).cyan().bold());
    println!("{}", style(format!("Building: {}", target)).cyan().bold());
    println!("{}\n", style(&separator).cyan().bold());
}

/// Print result after verbose build (sequential mode)
pub fn verbose_result(
    target: &str,
    success: bool,
    artifact: Option<&std::path::PathBuf>,
    duration: Option<Duration>,
) {
    let time_str = duration
        .map(|d| format!(" in {}", format_duration(d)))
        .unwrap_or_default();

    println!();
    if success {
        println!(
            "{}{}",
            style(format!("✓ {} succeeded", target)).green().bold(),
            time_str
        );
        if let Some(path) = artifact {
            println!("  Artifact: {}", path.display());
        }
    } else {
        println!(
            "{}{}",
            style(format!("✗ {} failed", target)).red().bold(),
            time_str
        );
    }
    println!();
}

/// Color palette for target prefixes (rotating)
const TARGET_COLORS: &[fn(&str) -> console::StyledObject<&str>] = &[
    |s| style(s).cyan(),
    |s| style(s).magenta(),
    |s| style(s).yellow(),
    |s| style(s).blue(),
    |s| style(s).green(),
];

/// Get a styled target prefix based on index
pub fn styled_target(target: &str, index: usize) -> String {
    let color_fn = TARGET_COLORS[index % TARGET_COLORS.len()];
    format!("{}", color_fn(&format!("[{}]", target)))
}

/// Print a line with colored target prefix (for parallel verbose mode)
pub fn verbose_line(target: &str, index: usize, line: &str) {
    println!("{} {}", styled_target(target, index), line);
}

/// Print a start marker for parallel verbose mode
pub fn verbose_start(target: &str, index: usize) {
    println!(
        "{} {}",
        styled_target(target, index),
        style("starting build...").dim()
    );
}

/// Print a result line with colored prefix (for parallel verbose mode)
pub fn verbose_done(
    target: &str,
    index: usize,
    success: bool,
    artifact: Option<&std::path::PathBuf>,
    duration: Option<Duration>,
) {
    let time_str = duration
        .map(|d| format!(" ({})", format_duration(d)))
        .unwrap_or_default();

    if success {
        let artifact_str = artifact
            .map(|p| format!(" → {}", p.file_name().unwrap_or_default().to_string_lossy()))
            .unwrap_or_default();
        println!(
            "{} {}{}{}",
            styled_target(target, index),
            style("✓ succeeded").green(),
            time_str,
            artifact_str
        );
    } else {
        println!(
            "{} {}{}",
            styled_target(target, index),
            style("✗ failed").red(),
            time_str
        );
    }
}

// === Legacy compatibility functions ===
// These maintain the old API while using console internally

/// Initialize the build progress display with target names (legacy - now no-op, use BuildProgress)
pub fn init_build_progress(_targets: &[String]) {
    // No-op - use BuildProgress::new() instead
}

/// Update a specific target's build status (legacy - now prints simple line)
pub fn update_build_status(target: &str, state: BuildState, progress: &str) {
    build_status(target, state, progress);
}

/// Finish the build progress display (legacy - now no-op)
pub fn finish_build_progress() {
    // No-op - use BuildProgress methods instead
}

/// Print a build target status (simple, non-updating version)
pub fn build_status(target: &str, state: BuildState, message: &str) {
    let (symbol, color_fn): (&str, fn(String) -> console::StyledObject<String>) = match state {
        BuildState::Pending => ("  ", |s| style(s).dim()),
        BuildState::Starting => ("..", |s| style(s).cyan()),
        BuildState::Running => (">>", |s| style(s).blue()),
        BuildState::Success => ("OK", |s| style(s).green()),
        BuildState::Failed => ("XX", |s| style(s).red()),
    };

    let prefix = color_fn(format!("[{}]", symbol));
    if message.is_empty() {
        println!("{} {}", prefix, target);
    } else {
        println!("{} {} {}", prefix, target, style(message).dim());
    }
}

// Re-export for compatibility
pub fn target_color(index: usize) -> usize {
    index
}
