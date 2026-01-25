use std::path::Path;
use std::process::Command;

use super::Runtime;

/// Builder for container run commands
#[allow(dead_code)]
pub struct ContainerCommand {
    runtime: Runtime,
    image: String,
    mounts: Vec<Mount>,
    workdir: Option<String>,
    env: Vec<(String, String)>,
    command: Vec<String>,
    remove: bool,
}

struct Mount {
    host_path: String,
    container_path: String,
    readonly: bool,
}

#[allow(dead_code)]
impl ContainerCommand {
    pub fn new(runtime: Runtime, image: impl Into<String>) -> Self {
        Self {
            runtime,
            image: image.into(),
            mounts: Vec::new(),
            workdir: None,
            env: Vec::new(),
            command: Vec::new(),
            remove: true,
        }
    }

    /// Add a volume mount
    pub fn mount(
        mut self,
        host_path: impl AsRef<Path>,
        container_path: impl Into<String>,
        readonly: bool,
    ) -> Self {
        self.mounts.push(Mount {
            host_path: host_path.as_ref().to_string_lossy().to_string(),
            container_path: container_path.into(),
            readonly,
        });
        self
    }

    /// Set the working directory inside the container
    pub fn workdir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Add an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Set the command to run
    pub fn command(mut self, cmd: Vec<String>) -> Self {
        self.command = cmd;
        self
    }

    /// Set the command from a shell string
    pub fn shell_command(mut self, cmd: impl Into<String>) -> Self {
        self.command = vec!["/bin/bash".to_string(), "-c".to_string(), cmd.into()];
        self
    }

    /// Don't remove container after exit (useful for debugging)
    pub fn keep(mut self) -> Self {
        self.remove = false;
        self
    }

    /// Build the Command
    pub fn build(&self) -> Command {
        let mut cmd = self.runtime.command();

        cmd.arg("run");

        if self.remove {
            cmd.arg("--rm");
        }

        // Add mounts
        for mount in &self.mounts {
            let mount_spec = if mount.readonly {
                format!("{}:{}:ro", mount.host_path, mount.container_path)
            } else {
                format!("{}:{}", mount.host_path, mount.container_path)
            };
            cmd.arg("-v").arg(mount_spec);
        }

        // Set working directory
        if let Some(ref workdir) = self.workdir {
            cmd.arg("-w").arg(workdir);
        }

        // Add environment variables
        for (key, value) in &self.env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        // Add image
        cmd.arg(&self.image);

        // Add command
        cmd.args(&self.command);

        cmd
    }

    /// Get the command as a string (for debugging/display)
    pub fn as_string(&self) -> String {
        let mut parts = vec![self.runtime.command_name().to_string(), "run".to_string()];

        if self.remove {
            parts.push("--rm".to_string());
        }

        for mount in &self.mounts {
            parts.push("-v".to_string());
            let mount_spec = if mount.readonly {
                format!("{}:{}:ro", mount.host_path, mount.container_path)
            } else {
                format!("{}:{}", mount.host_path, mount.container_path)
            };
            parts.push(mount_spec);
        }

        if let Some(ref workdir) = self.workdir {
            parts.push("-w".to_string());
            parts.push(workdir.clone());
        }

        for (key, value) in &self.env {
            parts.push("-e".to_string());
            parts.push(format!("{}={}", key, value));
        }

        parts.push(self.image.clone());
        parts.extend(self.command.clone());

        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_command_build() {
        let cmd = ContainerCommand::new(Runtime::Docker, "test-image")
            .mount("/host/path", "/container/path", false)
            .mount("/host/readonly", "/container/readonly", true)
            .workdir("/workspace")
            .env("FOO", "bar")
            .shell_command("echo hello");

        let s = cmd.as_string();
        assert!(s.contains("docker run"));
        assert!(s.contains("--rm"));
        assert!(s.contains("-v /host/path:/container/path"));
        assert!(s.contains("-v /host/readonly:/container/readonly:ro"));
        assert!(s.contains("-w /workspace"));
        assert!(s.contains("-e FOO=bar"));
        assert!(s.contains("test-image"));
        assert!(s.contains("echo hello"));
    }
}
