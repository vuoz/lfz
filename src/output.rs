//! Colored terminal output utilities

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Mutex;

// ANSI color codes
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";

// Colors
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";

// Bright colors for more variety
pub const BRIGHT_RED: &str = "\x1b[91m";
pub const BRIGHT_GREEN: &str = "\x1b[92m";
pub const BRIGHT_YELLOW: &str = "\x1b[93m";
pub const BRIGHT_BLUE: &str = "\x1b[94m";
pub const BRIGHT_MAGENTA: &str = "\x1b[95m";
pub const BRIGHT_CYAN: &str = "\x1b[96m";

/// Color palette for target prefixes (rotating)
const TARGET_COLORS: &[&str] = &[
    CYAN,
    MAGENTA,
    YELLOW,
    BLUE,
    GREEN,
    BRIGHT_CYAN,
    BRIGHT_MAGENTA,
    BRIGHT_YELLOW,
    BRIGHT_BLUE,
    BRIGHT_GREEN,
];

/// Get a color for a target based on its index
pub fn target_color(index: usize) -> &'static str {
    TARGET_COLORS[index % TARGET_COLORS.len()]
}

// ANSI escape sequences for cursor control
const CLEAR_LINE: &str = "\x1b[2K";
const CURSOR_UP: &str = "\x1b[A";
const CURSOR_DOWN: &str = "\x1b[B";
const CURSOR_START: &str = "\r";

/// Global state for tracking build progress lines
lazy_static::lazy_static! {
    static ref BUILD_TRACKER: Mutex<BuildTracker> = Mutex::new(BuildTracker::new());
}

struct BuildTracker {
    /// Map of target name to its current status
    targets: HashMap<String, TargetStatus>,
    /// Order of targets (for consistent display)
    order: Vec<String>,
    /// Whether we've printed the initial lines
    initialized: bool,
}

struct TargetStatus {
    state: BuildState,
    progress: String,
}

impl BuildTracker {
    fn new() -> Self {
        Self {
            targets: HashMap::new(),
            order: Vec::new(),
            initialized: false,
        }
    }

    fn reset(&mut self) {
        self.targets.clear();
        self.order.clear();
        self.initialized = false;
    }
}

/// Print a status message (cyan, bold prefix)
pub fn status(prefix: &str, message: &str) {
    println!("{BOLD}{CYAN}{prefix}{RESET} {message}");
}

/// Print an info message (blue)
pub fn info(message: &str) {
    println!("{BLUE}{message}{RESET}");
}

/// Print a success message (green)
pub fn success(message: &str) {
    println!("{GREEN}{message}{RESET}");
}

/// Print a warning message (yellow)
pub fn warning(message: &str) {
    println!("{YELLOW}warning:{RESET} {message}");
}

/// Print an error message (red)
pub fn error(message: &str) {
    eprintln!("{RED}error:{RESET} {message}");
}

/// Initialize the build progress display with target names
pub fn init_build_progress(targets: &[String]) {
    let mut tracker = BUILD_TRACKER.lock().unwrap();
    tracker.reset();

    for target in targets {
        tracker.order.push(target.clone());
        tracker.targets.insert(
            target.clone(),
            TargetStatus {
                state: BuildState::Pending,
                progress: String::new(),
            },
        );
    }

    // Print initial lines for each target (use print! for last to avoid extra newline)
    let len = tracker.order.len();
    for (i, target) in tracker.order.iter().enumerate() {
        if i < len - 1 {
            println!("{BOLD}{DIM}[  ]{RESET} {target} {DIM}waiting{RESET}");
        } else {
            print!("{BOLD}{DIM}[  ]{RESET} {target} {DIM}waiting{RESET}");
        }
    }
    tracker.initialized = true;
    let _ = io::stdout().flush();
}

/// Update a specific target's build status (updates in place)
pub fn update_build_status(target: &str, state: BuildState, progress: &str) {
    let mut tracker = BUILD_TRACKER.lock().unwrap();

    // Update the status
    if let Some(status) = tracker.targets.get_mut(target) {
        status.state = state;
        status.progress = progress.to_string();
    }

    if !tracker.initialized {
        // Fallback to simple output if not initialized
        drop(tracker);
        build_status(target, state, progress);
        return;
    }

    // Find this target's position (from bottom)
    // Cursor is at end of last line, so last target = 0 lines up
    let position = tracker.order.iter().position(|t| t == target).unwrap_or(0);
    let lines_up = tracker.order.len() - position - 1;

    // Move cursor up, clear line, print, move back down
    let mut output = String::new();

    // Move up to the target's line
    for _ in 0..lines_up {
        output.push_str(CURSOR_UP);
    }

    // Clear and rewrite the line
    output.push_str(CURSOR_START);
    output.push_str(CLEAR_LINE);
    output.push_str(&format_build_line(target, state, progress));

    // Move back down to the last line
    for _ in 0..lines_up {
        output.push_str(CURSOR_DOWN);
    }

    print!("{}", output);
    let _ = io::stdout().flush();
}

/// Finish the build progress display (move cursor below all targets)
pub fn finish_build_progress() {
    let mut tracker = BUILD_TRACKER.lock().unwrap();
    if tracker.initialized {
        println!(); // Move to next line after progress display
    }
    tracker.reset();
}

fn format_build_line(target: &str, state: BuildState, progress: &str) -> String {
    let (color, symbol) = match state {
        BuildState::Pending => (DIM, "  "),
        BuildState::Starting => (CYAN, ".."),
        BuildState::Running => (BLUE, ">>"),
        BuildState::Success => (GREEN, "OK"),
        BuildState::Failed => (RED, "XX"),
    };

    if progress.is_empty() {
        format!("{BOLD}{color}[{symbol}]{RESET} {target}")
    } else {
        format!("{BOLD}{color}[{symbol}]{RESET} {target} {DIM}{progress}{RESET}")
    }
}

/// Print a build target status (simple, non-updating version)
pub fn build_status(target: &str, state: BuildState, message: &str) {
    println!("{}", format_build_line(target, state, message));
}

/// Print a section header
pub fn header(message: &str) {
    println!("\n{BOLD}{MAGENTA}==> {message}{RESET}");
}

/// Print build error output with formatting
pub fn build_error_output(target: &str, output: &str) {
    println!("{DIM}--- Output for {target} ---{RESET}");

    // Print each line, highlighting errors
    for line in output.lines() {
        if line.contains("error:") || line.contains("Error") || line.contains("FATAL") {
            println!("{RED}{line}{RESET}");
        } else if line.contains("warning:") {
            println!("{YELLOW}{line}{RESET}");
        } else {
            println!("{DIM}{line}{RESET}");
        }
    }

    println!("{DIM}--- End output ---{RESET}");
}

/// Print a list item
pub fn list_item(item: &str) {
    println!("  {DIM}-{RESET} {item}");
}

/// Print a key-value pair
pub fn kv(key: &str, value: &str) {
    println!("  {DIM}{key}:{RESET} {value}");
}

/// Format a duration as human-readable string
pub fn format_duration(duration: std::time::Duration) -> String {
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
pub fn summary(succeeded: usize, failed: usize, total_time: Option<std::time::Duration>) {
    println!();
    let time_str = total_time
        .map(|d| format!(" in {}", format_duration(d)))
        .unwrap_or_default();

    if failed == 0 {
        println!(
            "{BOLD}{GREEN}Build complete:{RESET} {succeeded} succeeded, {failed} failed{time_str}"
        );
    } else {
        println!(
            "{BOLD}{RED}Build complete:{RESET} {GREEN}{succeeded} succeeded{RESET}, {RED}{failed} failed{RESET}{time_str}"
        );
    }
}

/// Print command being executed (dimmed)
pub fn command(cmd: &str) {
    println!("{DIM}$ {cmd}{RESET}");
}

/// Print a header for verbose build output (sequential mode)
pub fn verbose_header(target: &str) {
    let separator = "=".repeat(60);
    println!("\n{BOLD}{CYAN}{separator}{RESET}");
    println!("{BOLD}{CYAN}Building: {target}{RESET}");
    println!("{BOLD}{CYAN}{separator}{RESET}\n");
}

/// Print result after verbose build (sequential mode)
pub fn verbose_result(
    target: &str,
    success: bool,
    artifact: Option<&std::path::PathBuf>,
    duration: Option<std::time::Duration>,
) {
    let time_str = duration
        .map(|d| format!(" in {}", format_duration(d)))
        .unwrap_or_default();

    println!();
    if success {
        println!("{BOLD}{GREEN}✓ {target} succeeded{RESET}{time_str}");
        if let Some(path) = artifact {
            println!("  Artifact: {}", path.display());
        }
    } else {
        println!("{BOLD}{RED}✗ {target} failed{RESET}{time_str}");
    }
    println!();
}

/// Print a line with colored target prefix (for parallel verbose mode)
pub fn verbose_line(target: &str, color: &str, line: &str) {
    println!("{color}[{target}]{RESET} {line}");
}

/// Print a start marker for parallel verbose mode
pub fn verbose_start(target: &str, color: &str) {
    println!("{BOLD}{color}[{target}]{RESET} {DIM}starting build...{RESET}");
}

/// Print a result line with colored prefix (for parallel verbose mode)
pub fn verbose_done(
    target: &str,
    color: &str,
    success: bool,
    artifact: Option<&std::path::PathBuf>,
    duration: Option<std::time::Duration>,
) {
    let time_str = duration
        .map(|d| format!(" ({})", format_duration(d)))
        .unwrap_or_default();

    if success {
        print!("{BOLD}{color}[{target}]{RESET} {GREEN}✓ succeeded{RESET}{time_str}");
        if let Some(path) = artifact {
            println!(
                " → {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
        } else {
            println!();
        }
    } else {
        println!("{BOLD}{color}[{target}]{RESET} {RED}✗ failed{RESET}{time_str}");
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
