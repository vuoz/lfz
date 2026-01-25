use anyhow::Result;

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::artifacts::collect_artifact;
use super::target::BuildTarget;
use crate::config::project::Project;
use crate::container::{ContainerCommand, Runtime, DEFAULT_IMAGE};
use crate::output::{self, BuildProgress, BuildState};
use crate::paths;

/// Result of a single build
#[derive(Debug, Default)]
pub struct BuildResult {
    pub target_name: String,
    pub success: bool,
    pub error: Option<String>,
    pub error_output: Option<String>,
    pub artifact_path: Option<PathBuf>,
    pub duration: Option<Duration>,
}

/// Helper to create a failed BuildResult
fn failed_result(target_name: String, error: String) -> BuildResult {
    BuildResult {
        target_name,
        success: false,
        error: Some(error),
        ..Default::default()
    }
}

/// Orchestrates building multiple targets
pub struct BuildOrchestrator {
    runtime: Runtime,
    workspace: PathBuf,
    project: Project,
    output_dir: PathBuf,
    quiet: bool,
    verbose: bool,
    pristine: bool,
}

impl BuildOrchestrator {
    pub fn new(
        runtime: Runtime,
        workspace: PathBuf,
        project: Project,
        output_dir: PathBuf,
        quiet: bool,
        verbose: bool,
        pristine: bool,
    ) -> Self {
        Self {
            runtime,
            workspace,
            project,
            output_dir,
            quiet,
            verbose,
            pristine,
        }
    }

    /// Build targets sequentially
    pub fn build_sequential(&self, targets: &[BuildTarget]) -> Result<Vec<BuildResult>> {
        let mut results = Vec::new();

        for target in targets {
            let result = if self.verbose {
                self.build_target_verbose(target)
            } else {
                self.build_target(target)
            };
            results.push(result);
        }

        Ok(results)
    }

    /// Build targets in parallel using threads with optional concurrency limit
    pub fn build_parallel(
        &self,
        targets: &[BuildTarget],
        max_jobs: usize,
    ) -> Result<Vec<BuildResult>> {
        // Use verbose parallel mode if verbose flag is set
        if self.verbose {
            return self.build_parallel_verbose(targets, max_jobs);
        }

        // Hide cursor during progress display
        let term = console::Term::stderr();
        if !self.quiet {
            let _ = term.hide_cursor();
        }

        // Initialize the progress display with all target names
        let progress = if !self.quiet {
            let target_names: Vec<String> =
                targets.iter().map(|t| t.artifact_name.clone()).collect();
            Some(Arc::new(BuildProgress::new(&target_names)))
        } else {
            None
        };

        let results = Arc::new(Mutex::new(Vec::new()));
        let semaphore = Arc::new(Semaphore::new(max_jobs));
        let mut handles = Vec::new();

        for (index, target) in targets.iter().enumerate() {
            let target = target.clone();
            let runtime = self.runtime;
            let workspace = self.workspace.clone();
            let project_config_dir = self.project.config_dir.clone();
            let extra_modules = self.project.extra_modules();
            let output_dir = self.output_dir.clone();
            let pristine = self.pristine;
            let results = Arc::clone(&results);
            let semaphore = Arc::clone(&semaphore);
            let progress = progress.clone();

            let handle = thread::spawn(move || {
                // Acquire semaphore permit (blocks if max_jobs already running)
                let _permit = semaphore.acquire();

                let result = Self::build_target_with_progress(
                    &runtime,
                    &workspace,
                    &project_config_dir,
                    &extra_modules,
                    &output_dir,
                    &target,
                    pristine,
                    progress.as_ref().map(|p| (p.as_ref(), index)),
                );

                let mut results = results.lock().unwrap();
                results.push(result);

                // Permit is dropped here, allowing another thread to proceed
            });

            handles.push(handle);
        }

        // Wait for all builds to complete
        for handle in handles {
            handle.join().expect("Build thread panicked");
        }

        // Print final results to stdout
        if let Some(ref prog) = progress {
            prog.print_results();
        }

        // Restore cursor
        if !self.quiet {
            let _ = term.show_cursor();
        }

        let results = Arc::try_unwrap(results)
            .expect("Arc still has multiple owners")
            .into_inner()
            .unwrap();

        Ok(results)
    }

    /// Build targets in parallel with verbose streaming output (colored prefixes)
    fn build_parallel_verbose(
        &self,
        targets: &[BuildTarget],
        max_jobs: usize,
    ) -> Result<Vec<BuildResult>> {
        let results = Arc::new(Mutex::new(Vec::new()));
        let semaphore = Arc::new(Semaphore::new(max_jobs));
        let mut handles = Vec::new();

        for (index, target) in targets.iter().enumerate() {
            let target = target.clone();
            let runtime = self.runtime;
            let workspace = self.workspace.clone();
            let project_config_dir = self.project.config_dir.clone();
            let extra_modules = self.project.extra_modules();
            let output_dir = self.output_dir.clone();
            let pristine = self.pristine;
            let results = Arc::clone(&results);
            let semaphore = Arc::clone(&semaphore);

            let handle = thread::spawn(move || {
                // Acquire semaphore permit (blocks if max_jobs already running)
                let _permit = semaphore.acquire();

                let result = Self::build_target_verbose_parallel(
                    &runtime,
                    &workspace,
                    &project_config_dir,
                    &extra_modules,
                    &output_dir,
                    &target,
                    index,
                    pristine,
                );

                let mut results = results.lock().unwrap();
                results.push(result);
            });

            handles.push(handle);
        }

        // Wait for all builds to complete
        for handle in handles {
            handle.join().expect("Build thread panicked");
        }

        let results = Arc::try_unwrap(results)
            .expect("Arc still has multiple owners")
            .into_inner()
            .unwrap();

        Ok(results)
    }

    /// Build a single target
    fn build_target(&self, target: &BuildTarget) -> BuildResult {
        Self::build_target_inner(
            &self.runtime,
            &self.workspace,
            &self.project.config_dir,
            &self.project.extra_modules(),
            &self.output_dir,
            target,
            self.quiet,
            self.pristine,
        )
    }

    /// Build a single target with verbose streaming output
    fn build_target_verbose(&self, target: &BuildTarget) -> BuildResult {
        Self::build_target_verbose_inner(
            &self.runtime,
            &self.workspace,
            &self.project.config_dir,
            &self.project.extra_modules(),
            &self.output_dir,
            target,
            self.pristine,
        )
    }

    /// Inner build function - quiet during build, only prints final result
    fn build_target_inner(
        runtime: &Runtime,
        workspace: &PathBuf,
        config_dir: &PathBuf,
        extra_modules: &[PathBuf],
        output_dir: &PathBuf,
        target: &BuildTarget,
        quiet: bool,
        pristine: bool,
    ) -> BuildResult {
        let start = Instant::now();
        let target_name = target.artifact_name.clone();

        // Build the west build command
        let west_args = target.west_build_args("/workspace/config", pristine);
        let west_cmd = format!("west {}", west_args.join(" "));

        // Get ccache dir
        let ccache_dir = match paths::ccache_dir() {
            Ok(dir) => dir,
            Err(e) => {
                if !quiet {
                    output::build_status(&target_name, BuildState::Failed, "ccache error");
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to get ccache dir: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Build container command
        let mut container_cmd = ContainerCommand::new(*runtime, DEFAULT_IMAGE)
            .mount(workspace, "/workspace", false)
            .mount(config_dir, "/workspace/config", true)
            .mount(&ccache_dir, "/root/.ccache", false)
            .workdir("/workspace")
            .env(
                "CMAKE_PREFIX_PATH",
                "/workspace/zephyr/share/zephyr-package/cmake",
            );

        // Mount extra Zephyr modules
        for (i, module_path) in extra_modules.iter().enumerate() {
            let container_path = format!("/workspace/module_{}", i);
            container_cmd = container_cmd.mount(module_path, &container_path, true);
        }

        // Add ZMK_EXTRA_MODULES cmake arg if we have extra modules
        let module_paths: Vec<String> = (0..extra_modules.len())
            .map(|i| format!("/workspace/module_{}", i))
            .collect();

        let build_script = if module_paths.is_empty() {
            west_cmd
        } else {
            let modules_arg = module_paths.join(";");
            format!("{} -DZMK_EXTRA_MODULES=\"{}\"", west_cmd, modules_arg)
        };

        let mut cmd = container_cmd.shell_command(&build_script).build();

        // Capture output silently
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn the process
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                if !quiet {
                    output::build_status(&target_name, BuildState::Failed, "spawn error");
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to spawn build process: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Read stdout/stderr in background threads
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut all_output = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                all_output.push(line);
            }
            all_output.join("\n")
        });

        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut error_output = String::new();
            for line in reader.lines().map_while(Result::ok) {
                error_output.push_str(&line);
                error_output.push('\n');
            }
            error_output
        });

        // Wait for process to complete
        let status = match child.wait() {
            Ok(status) => status,
            Err(e) => {
                if !quiet {
                    output::build_status(&target_name, BuildState::Failed, "wait error");
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to wait for build: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        let stdout_output = stdout_handle.join().unwrap_or_default();
        let stderr_output = stderr_handle.join().unwrap_or_default();
        let duration = start.elapsed();

        if !status.success() {
            let mut combined_output = stdout_output;
            if !stderr_output.is_empty() {
                if !combined_output.is_empty() {
                    combined_output.push('\n');
                }
                combined_output.push_str(&stderr_output);
            }

            if !quiet {
                output::build_status(&target_name, BuildState::Failed, "error");
            }

            return BuildResult {
                target_name,
                success: false,
                error: Some(format!("Build failed with exit code: {:?}", status.code())),
                error_output: if combined_output.is_empty() {
                    None
                } else {
                    Some(combined_output)
                },
                artifact_path: None,
                duration: Some(duration),
            };
        }

        // Collect artifact
        match collect_artifact(workspace, target, output_dir) {
            Ok(artifact_path) => {
                if !quiet {
                    let artifact_name = artifact_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let time_str = output::format_duration(duration);
                    output::build_status(
                        &target_name,
                        BuildState::Success,
                        &format!("{} ({})", artifact_name, time_str),
                    );
                }
                BuildResult {
                    target_name,
                    success: true,
                    error: None,
                    error_output: None,
                    artifact_path: Some(artifact_path),
                    duration: Some(duration),
                }
            }
            Err(e) => {
                if !quiet {
                    output::build_status(&target_name, BuildState::Failed, "artifact error");
                }
                BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to collect artifact: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: Some(duration),
                }
            }
        }
    }

    /// Build a target with progress bar updates (for parallel non-verbose mode)
    fn build_target_with_progress(
        runtime: &Runtime,
        workspace: &PathBuf,
        config_dir: &PathBuf,
        extra_modules: &[PathBuf],
        output_dir: &PathBuf,
        target: &BuildTarget,
        pristine: bool,
        progress: Option<(&BuildProgress, usize)>,
    ) -> BuildResult {
        use std::sync::mpsc::{channel, TryRecvError};

        let start = Instant::now();
        let target_name = target.artifact_name.clone();

        if let Some((prog, idx)) = progress {
            prog.update(idx, BuildState::Starting, "configuring");
        }

        // Build the west build command
        let west_args = target.west_build_args("/workspace/config", pristine);
        let west_cmd = format!("west {}", west_args.join(" "));

        // Get ccache dir
        let ccache_dir = match paths::ccache_dir() {
            Ok(dir) => dir,
            Err(e) => {
                if let Some((prog, idx)) = progress {
                    prog.finish(idx, false, None, None);
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to get ccache dir: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Build container command
        let mut container_cmd = ContainerCommand::new(*runtime, DEFAULT_IMAGE)
            .mount(workspace, "/workspace", false)
            .mount(config_dir, "/workspace/config", true)
            .mount(&ccache_dir, "/root/.ccache", false)
            .workdir("/workspace")
            .env(
                "CMAKE_PREFIX_PATH",
                "/workspace/zephyr/share/zephyr-package/cmake",
            );

        // Mount extra Zephyr modules
        for (i, module_path) in extra_modules.iter().enumerate() {
            let container_path = format!("/workspace/module_{}", i);
            container_cmd = container_cmd.mount(module_path, &container_path, true);
        }

        // Add ZMK_EXTRA_MODULES cmake arg if we have extra modules
        let module_paths: Vec<String> = (0..extra_modules.len())
            .map(|i| format!("/workspace/module_{}", i))
            .collect();

        let build_script = if module_paths.is_empty() {
            west_cmd
        } else {
            let modules_arg = module_paths.join(";");
            format!("{} -DZMK_EXTRA_MODULES=\"{}\"", west_cmd, modules_arg)
        };

        let mut cmd = container_cmd.shell_command(&build_script).build();

        // Set up for streaming output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn the process
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                if let Some((prog, idx)) = progress {
                    prog.finish(idx, false, None, None);
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to spawn build process: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Set up channels for progress updates
        let (progress_tx, progress_rx) = channel::<String>();

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        // Spawn thread to read stdout, parse progress, and capture output
        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut all_output: Vec<String> = Vec::new();

            for line in reader.lines().map_while(Result::ok) {
                all_output.push(line.clone());

                // Parse ninja progress like [123/456]
                if let Some((current, total, _phase)) = parse_build_progress(&line) {
                    // Send progress update as [current/total]
                    let msg = format!("[{}/{}]", current, total);
                    let _ = progress_tx.send(msg); // Ignore send errors
                }
            }

            all_output.join("\n")
        });

        // Spawn thread to read stderr
        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut error_output = String::new();
            for line in reader.lines().map_while(Result::ok) {
                error_output.push_str(&line);
                error_output.push('\n');
            }
            error_output
        });

        // Poll for progress updates while waiting for process to complete
        let status = loop {
            // Process any pending progress updates
            if let Some((prog, idx)) = progress {
                loop {
                    match progress_rx.try_recv() {
                        Ok(msg) => prog.update(idx, BuildState::Running, &msg),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => break,
                    }
                }
            }

            // Check if process is done
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    // Process still running, sleep briefly
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => break Err(e),
            }
        };

        let status = match status {
            Ok(status) => status,
            Err(e) => {
                if let Some((prog, idx)) = progress {
                    prog.finish(idx, false, None, None);
                }
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to wait for build: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Get output from threads
        let stdout_output = stdout_handle.join().unwrap_or_default();
        let stderr_output = stderr_handle.join().unwrap_or_default();

        let duration = start.elapsed();

        if !status.success() {
            if let Some((prog, idx)) = progress {
                prog.finish(idx, false, None, Some(duration));
            }

            // Combine stdout and stderr for the error output
            let mut combined_output = stdout_output.clone();
            if !stderr_output.is_empty() {
                if !combined_output.is_empty() {
                    combined_output.push('\n');
                }
                combined_output.push_str(&stderr_output);
            }

            return BuildResult {
                target_name,
                success: false,
                error: Some(format!("Build failed with exit code: {:?}", status.code())),
                error_output: if combined_output.is_empty() {
                    None
                } else {
                    Some(combined_output)
                },
                artifact_path: None,
                duration: Some(duration),
            };
        }

        // Collect artifact
        match collect_artifact(workspace, target, output_dir) {
            Ok(artifact_path) => {
                let artifact_name = artifact_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                if let Some((prog, idx)) = progress {
                    prog.finish(idx, true, Some(&artifact_name), Some(duration));
                }

                BuildResult {
                    target_name,
                    success: true,
                    error: None,
                    error_output: None,
                    artifact_path: Some(artifact_path),
                    duration: Some(duration),
                }
            }
            Err(e) => {
                if let Some((prog, idx)) = progress {
                    prog.finish(idx, false, None, Some(duration));
                }
                BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to collect artifact: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: Some(duration),
                }
            }
        }
    }

    /// Build with verbose streaming output and colored prefix (for parallel verbose mode)
    fn build_target_verbose_parallel(
        runtime: &Runtime,
        workspace: &PathBuf,
        config_dir: &PathBuf,
        extra_modules: &[PathBuf],
        output_dir: &PathBuf,
        target: &BuildTarget,
        color_index: usize,
        pristine: bool,
    ) -> BuildResult {
        let start = Instant::now();
        let target_name = target.artifact_name.clone();

        output::verbose_start(&target_name, color_index);

        // Build the west build command
        let west_args = target.west_build_args("/workspace/config", pristine);
        let west_cmd = format!("west {}", west_args.join(" "));

        // Get ccache dir
        let ccache_dir = match paths::ccache_dir() {
            Ok(dir) => dir,
            Err(e) => {
                output::verbose_line(
                    &target_name,
                    color_index,
                    &format!("error: Failed to get ccache dir: {}", e),
                );
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to get ccache dir: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Build container command
        let mut container_cmd = ContainerCommand::new(*runtime, DEFAULT_IMAGE)
            .mount(workspace, "/workspace", false)
            .mount(config_dir, "/workspace/config", true)
            .mount(&ccache_dir, "/root/.ccache", false)
            .workdir("/workspace")
            .env(
                "CMAKE_PREFIX_PATH",
                "/workspace/zephyr/share/zephyr-package/cmake",
            );

        // Mount extra Zephyr modules
        for (i, module_path) in extra_modules.iter().enumerate() {
            let container_path = format!("/workspace/module_{}", i);
            container_cmd = container_cmd.mount(module_path, &container_path, true);
        }

        // Add ZMK_EXTRA_MODULES cmake arg if we have extra modules
        let module_paths: Vec<String> = (0..extra_modules.len())
            .map(|i| format!("/workspace/module_{}", i))
            .collect();

        let build_script = if module_paths.is_empty() {
            west_cmd
        } else {
            let modules_arg = module_paths.join(";");
            format!("{} -DZMK_EXTRA_MODULES=\"{}\"", west_cmd, modules_arg)
        };

        let mut cmd = container_cmd.shell_command(&build_script).build();

        // Capture stdout/stderr for prefixing
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn the process
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                output::verbose_line(
                    &target_name,
                    color_index,
                    &format!("error: Failed to spawn: {}", e),
                );
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to spawn build process: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let target_name_stdout = target_name.clone();
        let target_name_stderr = target_name.clone();

        // Stream stdout with prefix
        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                output::verbose_line(&target_name_stdout, color_index, &line);
            }
        });

        // Stream stderr with prefix
        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                output::verbose_line(&target_name_stderr, color_index, &line);
            }
        });

        // Wait for output threads
        let _ = stdout_handle.join();
        let _ = stderr_handle.join();

        // Wait for process
        let status = match child.wait() {
            Ok(status) => status,
            Err(e) => {
                let duration = start.elapsed();
                output::verbose_done(&target_name, color_index, false, None, Some(duration));
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to wait for build: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: Some(duration),
                };
            }
        };

        let duration = start.elapsed();

        if !status.success() {
            output::verbose_done(&target_name, color_index, false, None, Some(duration));
            return BuildResult {
                target_name,
                success: false,
                error: Some(format!("Build failed with exit code: {:?}", status.code())),
                error_output: None,
                artifact_path: None,
                duration: Some(duration),
            };
        }

        // Collect artifact
        match collect_artifact(workspace, target, output_dir) {
            Ok(artifact_path) => {
                output::verbose_done(
                    &target_name,
                    color_index,
                    true,
                    Some(&artifact_path),
                    Some(duration),
                );
                BuildResult {
                    target_name,
                    success: true,
                    error: None,
                    error_output: None,
                    artifact_path: Some(artifact_path),
                    duration: Some(duration),
                }
            }
            Err(e) => {
                output::verbose_line(
                    &target_name,
                    color_index,
                    &format!("error: Failed to collect artifact: {}", e),
                );
                output::verbose_done(&target_name, color_index, false, None, Some(duration));
                BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to collect artifact: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: Some(duration),
                }
            }
        }
    }

    /// Build with verbose streaming output - shows all build output in real-time (sequential)
    fn build_target_verbose_inner(
        runtime: &Runtime,
        workspace: &PathBuf,
        config_dir: &PathBuf,
        extra_modules: &[PathBuf],
        output_dir: &PathBuf,
        target: &BuildTarget,
        pristine: bool,
    ) -> BuildResult {
        let start = Instant::now();
        let target_name = target.artifact_name.clone();

        // Print header for this target
        output::verbose_header(&target_name);

        // Build the west build command
        let west_args = target.west_build_args("/workspace/config", pristine);
        let west_cmd = format!("west {}", west_args.join(" "));

        output::command(&west_cmd);
        println!();

        // Get ccache dir
        let ccache_dir = match paths::ccache_dir() {
            Ok(dir) => dir,
            Err(e) => {
                output::error(&format!("Failed to get ccache dir: {}", e));
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to get ccache dir: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        // Build container command
        let mut container_cmd = ContainerCommand::new(*runtime, DEFAULT_IMAGE)
            .mount(workspace, "/workspace", false)
            .mount(config_dir, "/workspace/config", true)
            .mount(&ccache_dir, "/root/.ccache", false)
            .workdir("/workspace")
            .env(
                "CMAKE_PREFIX_PATH",
                "/workspace/zephyr/share/zephyr-package/cmake",
            );

        // Mount extra Zephyr modules
        for (i, module_path) in extra_modules.iter().enumerate() {
            let container_path = format!("/workspace/module_{}", i);
            container_cmd = container_cmd.mount(module_path, &container_path, true);
        }

        // Add ZMK_EXTRA_MODULES cmake arg if we have extra modules
        let module_paths: Vec<String> = (0..extra_modules.len())
            .map(|i| format!("/workspace/module_{}", i))
            .collect();

        let build_script = if module_paths.is_empty() {
            west_cmd
        } else {
            let modules_arg = module_paths.join(";");
            format!("{} -DZMK_EXTRA_MODULES=\"{}\"", west_cmd, modules_arg)
        };

        let mut cmd = container_cmd.shell_command(&build_script).build();

        // Inherit stdout/stderr for real-time streaming
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        // Run the build
        let status = match cmd.status() {
            Ok(status) => status,
            Err(e) => {
                output::error(&format!("Failed to run build: {}", e));
                return BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to run build: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: None,
                };
            }
        };

        println!();

        let duration = start.elapsed();

        if !status.success() {
            output::verbose_result(&target_name, false, None, Some(duration));
            return BuildResult {
                target_name,
                success: false,
                error: Some(format!("Build failed with exit code: {:?}", status.code())),
                error_output: None,
                artifact_path: None,
                duration: Some(duration),
            };
        }

        // Collect artifact
        match collect_artifact(workspace, target, output_dir) {
            Ok(artifact_path) => {
                output::verbose_result(&target_name, true, Some(&artifact_path), Some(duration));
                BuildResult {
                    target_name,
                    success: true,
                    error: None,
                    error_output: None,
                    artifact_path: Some(artifact_path),
                    duration: Some(duration),
                }
            }
            Err(e) => {
                output::error(&format!("Failed to collect artifact: {}", e));
                BuildResult {
                    target_name,
                    success: false,
                    error: Some(format!("Failed to collect artifact: {}", e)),
                    error_output: None,
                    artifact_path: None,
                    duration: Some(duration),
                }
            }
        }
    }
}

/// Parse ninja-style build progress like "[123/456] Building..."
/// Returns (current, total, optional_phase_name)
fn parse_build_progress(line: &str) -> Option<(usize, usize, Option<String>)> {
    let line = line.trim();

    // Match [current/total] pattern
    if line.starts_with('[') {
        if let Some(bracket_end) = line.find(']') {
            let progress = &line[1..bracket_end];
            if let Some(slash_pos) = progress.find('/') {
                let current: usize = progress[..slash_pos].parse().ok()?;
                let total: usize = progress[slash_pos + 1..].parse().ok()?;

                // Check for special phases
                let rest = &line[bracket_end + 1..];
                let phase = if rest.contains("Linking") {
                    Some("linking".to_string())
                } else if rest.contains("Generating") {
                    Some("generating".to_string())
                } else {
                    None
                };

                return Some((current, total, phase));
            }
        }
    }

    None
}

/// A simple counting semaphore for limiting concurrency
struct Semaphore {
    count: Mutex<usize>,
    condvar: Condvar,
}

impl Semaphore {
    fn new(count: usize) -> Self {
        Self {
            count: Mutex::new(count),
            condvar: Condvar::new(),
        }
    }

    fn acquire(&self) -> SemaphorePermit<'_> {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.condvar.wait(count).unwrap();
        }
        *count -= 1;
        SemaphorePermit { semaphore: self }
    }
}

/// RAII guard that releases the semaphore when dropped
struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        let mut count = self.semaphore.count.lock().unwrap();
        *count += 1;
        self.semaphore.condvar.notify_one();
    }
}
