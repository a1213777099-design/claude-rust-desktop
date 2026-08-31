/// Sandbox isolation for agent task execution.
///
/// Each agent task gets its own sandbox directory for file operations.
/// After review approval, files can be promoted to the real workspace.
/// This prevents agents from interfering with each other's work.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// A sandbox is an isolated working directory for a single agent task.
#[derive(Debug, Clone)]
pub struct Sandbox {
    /// The sandbox root directory (temporary, per-task).
    pub root: PathBuf,
    /// The real workspace path (for Read access).
    pub workspace: PathBuf,
    /// Task ID for tracking.
    pub task_id: String,
}

impl Sandbox {
    /// Create a new sandbox for a task.
    /// - Creates `sandbox/{task_id}/` under the base directory
    /// - Copies workspace structure references (symlinks not needed, Read goes to real workspace)
    pub fn create(base_dir: &Path, workspace: &Path, task_id: &str) -> Result<Self> {
        let root = base_dir.join("sandbox").join(task_id);
        std::fs::create_dir_all(&root)?;

        // Create standard subdirectories
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("outputs"))?;

        tracing::info!(target: "sandbox", "Created sandbox for task '{}' at {:?}", task_id, root);

        Ok(Self {
            root,
            workspace: workspace.to_path_buf(),
            task_id: task_id.to_string(),
        })
    }

    /// Get the effective path for a tool operation.
    /// - Read: resolve against real workspace first, then sandbox
    /// - Write/Edit: resolve against sandbox (isolated)
    /// - Bash: cwd is sandbox root
    /// - Glob/Grep: search both workspace and sandbox
    pub fn resolve_path(&self, tool_name: &str, path: &str) -> PathBuf {
        let clean_path = path.trim_start_matches(|c| c == '/' || c == '\\' || c == '.' );

        match tool_name {
            "Write" | "Edit" | "MultiEdit" => {
                // Writes go to sandbox
                self.root.join(clean_path)
            }
            "Read" => {
                // Read from workspace first (for existing files), then sandbox
                let ws_path = self.workspace.join(clean_path);
                if ws_path.exists() {
                    ws_path
                } else {
                    self.root.join(clean_path)
                }
            }
            "Bash" => {
                // Bash runs in workspace (so agents can access project files)
                self.workspace.clone()
            }
            "Glob" | "Grep" | "ListDir" => {
                // Search workspace (read-only)
                if path.is_empty() || path == "." {
                    self.workspace.clone()
                } else {
                    let ws_path = self.workspace.join(clean_path);
                    if ws_path.exists() {
                        ws_path
                    } else {
                        self.root.join(clean_path)
                    }
                }
            }
            _ => self.root.join(clean_path),
        }
    }

    /// Get the effective cwd for Bash commands.
    pub fn bash_cwd(&self) -> PathBuf {
        self.root.clone()
    }

    /// Check if a path is inside the sandbox (for safety).
    pub fn is_sandboxed(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    /// List files that were created/modified in the sandbox.
    pub fn list_changes(&self) -> Result<Vec<PathBuf>> {
        let mut changes = Vec::new();
        Self::collect_files_recursive(&self.root, &self.root, &mut changes)?;
        Ok(changes)
    }

    fn collect_files_recursive(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.exists() { return Ok(()); }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files_recursive(base, &path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }

    /// Promote sandbox files to the real workspace.
    /// Copies all files from sandbox to workspace, overwriting existing files.
    pub fn promote_to_workspace(&self) -> Result<Vec<PathBuf>> {
        let mut promoted = Vec::new();
        Self::copy_recursive(&self.root, &self.root, &self.workspace, &mut promoted)?;
        tracing::info!(target: "sandbox", "Promoted {} files from sandbox '{}' to workspace", promoted.len(), self.task_id);
        Ok(promoted)
    }

    fn copy_recursive(sandbox_root: &Path, src_dir: &Path, dest_base: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        if !src_dir.exists() { return Ok(()); }
        for entry in std::fs::read_dir(src_dir)? {
            let entry = entry?;
            let src = entry.path();
            // Compute relative path from sandbox root
            let rel = src.strip_prefix(sandbox_root).unwrap_or(&src);
            let dest = dest_base.join(rel);

            if src.is_dir() {
                std::fs::create_dir_all(&dest)?;
                Self::copy_recursive(sandbox_root, &src, dest_base, out)?;
            } else {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src, &dest)?;
                out.push(dest);
            }
        }
        Ok(())
    }

    /// Clean up the sandbox directory.
    pub fn cleanup(&self) -> Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
            tracing::info!(target: "sandbox", "Cleaned up sandbox for task '{}'", self.task_id);
        }
        Ok(())
    }
}

/// Global sandbox base directory.
pub fn get_sandbox_base(data_dir: &Path) -> PathBuf {
    data_dir.join("agent_sandboxes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sandbox_create_and_cleanup() {
        let tmp = std::env::temp_dir().join("sandbox_test_create");
        let _ = fs::remove_dir_all(&tmp);

        let sandbox = Sandbox::create(&tmp, &tmp.join("workspace"), "test-task-1").unwrap();
        assert!(sandbox.root.exists());
        assert!(sandbox.root.join("src").exists());
        assert!(sandbox.root.join("outputs").exists());

        sandbox.cleanup().unwrap();
        assert!(!sandbox.root.exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_path_write_goes_to_sandbox() {
        let tmp = std::env::temp_dir().join("sandbox_test_write");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("workspace")).unwrap();

        let sandbox = Sandbox::create(&tmp, &tmp.join("workspace"), "test-task-2").unwrap();

        let path = sandbox.resolve_path("Write", "src/main.rs");
        assert!(path.starts_with(&sandbox.root));

        let path = sandbox.resolve_path("Edit", "src/lib.rs");
        assert!(path.starts_with(&sandbox.root));

        sandbox.cleanup().unwrap();
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_path_read_from_workspace() {
        let tmp = std::env::temp_dir().join("sandbox_test_read");
        let _ = fs::remove_dir_all(&tmp);
        let ws = tmp.join("workspace");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("existing.txt"), "hello").unwrap();

        let sandbox = Sandbox::create(&tmp, &ws, "test-task-3").unwrap();

        // Should resolve to workspace (file exists there)
        let path = sandbox.resolve_path("Read", "existing.txt");
        assert_eq!(path, ws.join("existing.txt"));

        // Should resolve to sandbox (file doesn't exist in workspace)
        let path = sandbox.resolve_path("Read", "new_file.txt");
        assert!(path.starts_with(&sandbox.root));

        sandbox.cleanup().unwrap();
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_sandbox_isolation() {
        let tmp = std::env::temp_dir().join("sandbox_test_isolation");
        let _ = fs::remove_dir_all(&tmp);
        let ws = tmp.join("workspace");
        fs::create_dir_all(&ws).unwrap();

        let s1 = Sandbox::create(&tmp, &ws, "task-a").unwrap();
        let s2 = Sandbox::create(&tmp, &ws, "task-b").unwrap();

        // Sandboxes are independent
        assert_ne!(s1.root, s2.root);
        assert!(!s1.root.starts_with(&s2.root));
        assert!(!s2.root.starts_with(&s1.root));

        s1.cleanup().unwrap();
        s2.cleanup().unwrap();
        let _ = fs::remove_dir_all(&tmp);
    }
}
