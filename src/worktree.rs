//! worktree.rs - Git worktree management with task isolation
//!
//! Directory-level isolation for parallel task execution.
//! Tasks are the control plane and worktrees are the execution plane.
//!
//! ```text
//!     .tasks/task_12.json
//!       {
//!         "id": 12,
//!         "subject": "Implement auth refactor",
//!         "status": "in_progress",
//!         "worktree": "auth-refactor"
//!       }
//!
//!     .worktrees/index.json
//!       {
//!         "worktrees": [
//!           {
//!             "name": "auth-refactor",
//!             "path": ".../.worktrees/auth-refactor",
//!             "branch": "wt/auth-refactor",
//!             "task_id": 12,
//!             "status": "active"
//!           }
//!         ]
//!       }
//!
//! Key insight: "Isolate by directory, coordinate by task ID."
//!
//! Uses `git2` crate for programmatic git operations.

use git2::{Repository, StatusOptions};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A worktree entry in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub name: String,
    pub path: String,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<u32>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_at: Option<f64>,
}

/// A lifecycle event for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEvent {
    pub event: String,
    pub ts: f64,
    #[serde(default)]
    pub task: serde_json::Value,
    #[serde(default)]
    pub worktree: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Append-only event bus for lifecycle events.
pub struct EventBus {
    path: PathBuf,
}

impl EventBus {
    pub fn new(event_log_path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = event_log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !event_log_path.exists() {
            fs::write(event_log_path, "")?;
        }
        Ok(Self {
            path: event_log_path.to_path_buf(),
        })
    }

    pub fn emit(
        &self,
        event: &str,
        task: Option<serde_json::Value>,
        worktree: Option<serde_json::Value>,
        error: Option<&str>,
    ) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let payload = serde_json::json!({
            "event": event,
            "ts": ts,
            "task": task.unwrap_or(serde_json::json!({})),
            "worktree": worktree.unwrap_or(serde_json::json!({})),
            "error": error,
        });
        if let Ok(mut f) = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
        {
            let _ = std::io::Write::write_all(
                &mut f,
                format!("{}\n", serde_json::to_string(&payload).unwrap_or_default()).as_bytes(),
            );
        }
    }

    pub fn list_recent(&self, limit: usize) -> Result<String, String> {
        let content = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > limit {
            lines.len() - limit
        } else {
            0
        };
        let mut items = Vec::new();
        for line in &lines[start..] {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(v) => items.push(v),
                Err(_) => items.push(serde_json::json!({
                    "event": "parse_error",
                    "raw": line
                })),
            }
        }
        serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
    }
}

/// Index for tracking worktrees.
pub struct WorktreeIndex {
    path: PathBuf,
}

impl WorktreeIndex {
    pub fn new(index_path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !index_path.exists() {
            fs::write(
                index_path,
                serde_json::to_string_pretty(&serde_json::json!({"worktrees": []}))
                    .unwrap_or_default(),
            )?;
        }
        Ok(Self {
            path: index_path.to_path_buf(),
        })
    }

    fn load(&self) -> Result<Vec<WorktreeEntry>, String> {
        let content = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let data: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let entries: Vec<WorktreeEntry> =
            serde_json::from_value(data["worktrees"].clone()).map_err(|e| e.to_string())?;
        Ok(entries)
    }

    fn save(&self, entries: &[WorktreeEntry]) -> Result<(), String> {
        let data = serde_json::json!({"worktrees": entries});
        let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        fs::write(&self.path, content).map_err(|e| e.to_string())
    }

    pub fn find(&self, name: &str) -> Result<Option<WorktreeEntry>, String> {
        let entries = self.load()?;
        Ok(entries.into_iter().find(|e| e.name == name))
    }

    pub fn add(&self, entry: &WorktreeEntry) -> Result<(), String> {
        let mut entries = self.load()?;
        if entries.iter().any(|e| e.name == entry.name) {
            return Err(format!("Worktree '{}' already exists in index", entry.name));
        }
        entries.push(entry.clone());
        self.save(&entries)
    }

    pub fn update_status(&self, name: &str, status: &str) -> Result<(), String> {
        let mut entries = self.load()?;
        for entry in &mut entries {
            if entry.name == name {
                entry.status = status.to_string();
                if status == "removed" {
                    entry.removed_at = Some(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64(),
                    );
                } else if status == "kept" {
                    entry.kept_at = Some(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64(),
                    );
                }
            }
        }
        self.save(&entries)
    }

    pub fn list_all(&self) -> Result<String, String> {
        let entries = self.load()?;
        if entries.is_empty() {
            return Ok("No worktrees in index.".to_string());
        }
        let lines: Vec<String> = entries
            .iter()
            .map(|wt| {
                let task_suffix = wt
                    .task_id
                    .map(|id| format!(" task={id}"))
                    .unwrap_or_default();
                format!(
                    "[{}] {} -> {} ({}){}",
                    wt.status, wt.name, wt.path, wt.branch, task_suffix
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// Task binding for worktrees.
pub struct TaskBinding {
    tasks_dir: PathBuf,
}

impl TaskBinding {
    pub fn new(tasks_dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(tasks_dir)?;
        Ok(Self {
            tasks_dir: tasks_dir.to_path_buf(),
        })
    }

    pub fn bind(&self, task_id: u32, worktree_name: &str) -> Result<(), String> {
        let path = self.tasks_dir.join(format!("task_{task_id}.json"));
        if !path.exists() {
            return Err(format!("Task {task_id} not found"));
        }
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut task: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        task["worktree"] = serde_json::Value::String(worktree_name.to_string());
        if task["status"] == "pending" {
            task["status"] = serde_json::Value::String("in_progress".to_string());
        }
        let updated = serde_json::to_string_pretty(&task).map_err(|e| e.to_string())?;
        fs::write(&path, updated).map_err(|e| e.to_string())
    }

    pub fn unbind(&self, task_id: u32) -> Result<(), String> {
        let path = self.tasks_dir.join(format!("task_{task_id}.json"));
        if !path.exists() {
            return Err(format!("Task {task_id} not found"));
        }
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut task: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        task["worktree"] = serde_json::Value::String(String::new());
        let updated = serde_json::to_string_pretty(&task).map_err(|e| e.to_string())?;
        fs::write(&path, updated).map_err(|e| e.to_string())
    }

    pub fn complete(&self, task_id: u32) -> Result<(), String> {
        let path = self.tasks_dir.join(format!("task_{task_id}.json"));
        if !path.exists() {
            return Err(format!("Task {task_id} not found"));
        }
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut task: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        task["status"] = serde_json::Value::String("completed".to_string());
        task["worktree"] = serde_json::Value::String(String::new());
        let updated = serde_json::to_string_pretty(&task).map_err(|e| e.to_string())?;
        fs::write(&path, updated).map_err(|e| e.to_string())
    }
}

/// Detect if a directory is inside a git repository.
pub fn detect_repo_root(cwd: &Path) -> Option<PathBuf> {
    let repo = Repository::discover(cwd).ok()?;
    repo.workdir().map(|p| p.to_path_buf())
}

/// Validate a worktree name (1-40 alphanumeric chars, dots, underscores, hyphens).
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 40 {
        return Err("Invalid worktree name. Use 1-40 chars.".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err("Invalid worktree name. Use letters, numbers, ., _, -".to_string());
    }
    Ok(())
}

/// WorktreeManager: create/list/remove git worktrees + lifecycle index.
pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    index: WorktreeIndex,
    events: EventBus,
    git_available: bool,
}

impl WorktreeManager {
    pub fn new(repo_root: &Path) -> std::io::Result<Self> {
        let worktrees_dir = repo_root.join(".worktrees");
        fs::create_dir_all(&worktrees_dir)?;

        let index = WorktreeIndex::new(&worktrees_dir.join("index.json"))?;
        let events = EventBus::new(&worktrees_dir.join("events.jsonl"))?;
        let git_available = Repository::discover(repo_root).is_ok();

        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            worktrees_dir,
            index,
            events,
            git_available,
        })
    }

    pub fn is_git_available(&self) -> bool {
        self.git_available
    }

    /// Get git status for a worktree by name.
    pub fn status(&self, name: &str) -> Result<String, String> {
        let entry = self
            .index
            .find(name)?
            .ok_or_else(|| format!("Unknown worktree '{name}'"))?;
        let path = Path::new(&entry.path);
        if !path.exists() {
            return Err(format!("Worktree path missing: {}", path.display()));
        }
        let repo = Repository::open(path).map_err(|e| format!("Not a git repo: {e}"))?;
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("Failed to get status: {e}"))?;
        if statuses.is_empty() {
            Ok("Clean worktree".to_string())
        } else {
            Ok(format!("{} file(s) changed", statuses.len()))
        }
    }

    /// Create a worktree, optionally bound to a task.
    pub fn create(
        &self,
        name: &str,
        task_id: Option<u32>,
        _base_ref: &str,
    ) -> Result<String, String> {
        validate_name(name)?;

        if !self.git_available {
            return Err("Not in a git repository. worktree tools require git.".to_string());
        }

        if self.index.find(name)?.is_some() {
            return Err(format!("Worktree '{name}' already exists in index"));
        }

        let path = self.worktrees_dir.join(name);
        let branch = format!("wt/{name}");

        self.events.emit(
            "worktree.create.before",
            task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({"name": name, "base_ref": _base_ref})),
            None,
        );

        // Create the worktree via git command (git2 worktree API is complex)
        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                path.to_str().unwrap_or(""),
            ])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            self.events.emit(
                "worktree.create.failed",
                task_id.map(|id| serde_json::json!({"id": id})),
                Some(serde_json::json!({"name": name})),
                Some(&err),
            );
            return Err(format!("git worktree add failed: {err}"));
        }

        let entry = WorktreeEntry {
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            branch,
            task_id,
            status: "active".to_string(),
            created_at: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64(),
            ),
            removed_at: None,
            kept_at: None,
        };

        self.index.add(&entry)?;

        if let Some(tid) = task_id {
            let tasks_dir = self.repo_root.join(".tasks");
            let binding = TaskBinding::new(&tasks_dir).map_err(|e| e.to_string())?;
            binding.bind(tid, name)?;
        }

        self.events.emit(
            "worktree.create.after",
            task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({
                "name": name,
                "path": entry.path,
                "branch": entry.branch,
                "status": "active",
            })),
            None,
        );

        serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())
    }

    /// Remove a worktree and optionally complete its bound task.
    pub fn remove(&self, name: &str, force: bool, complete_task: bool) -> Result<String, String> {
        let entry = self
            .index
            .find(name)?
            .ok_or_else(|| format!("Unknown worktree '{name}'"))?;

        self.events.emit(
            "worktree.remove.before",
            entry.task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({"name": name, "path": entry.path})),
            None,
        );

        // Remove via git
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&entry.path);

        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git: {e}"))?;

        if !output.status.success() && !force {
            let err = String::from_utf8_lossy(&output.stderr);
            self.events.emit(
                "worktree.remove.failed",
                entry.task_id.map(|id| serde_json::json!({"id": id})),
                Some(serde_json::json!({"name": name, "path": entry.path})),
                Some(&err),
            );
            return Err(format!("git worktree remove failed: {err}"));
        }

        if complete_task {
            if let Some(tid) = entry.task_id {
                let tasks_dir = self.repo_root.join(".tasks");
                let binding = TaskBinding::new(&tasks_dir).map_err(|e| e.to_string())?;
                binding.complete(tid)?;
                self.events.emit(
                    "task.completed",
                    Some(serde_json::json!({"id": tid, "status": "completed"})),
                    Some(serde_json::json!({"name": name})),
                    None,
                );
            }
        }

        self.index.update_status(name, "removed")?;

        self.events.emit(
            "worktree.remove.after",
            entry.task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({"name": name, "path": entry.path, "status": "removed"})),
            None,
        );

        Ok(format!("Removed worktree '{name}'"))
    }

    /// Mark a worktree as kept without removing it.
    pub fn keep(&self, name: &str) -> Result<String, String> {
        let entry = self
            .index
            .find(name)?
            .ok_or_else(|| format!("Unknown worktree '{name}'"))?;

        self.index.update_status(name, "kept")?;

        self.events.emit(
            "worktree.keep",
            entry.task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({
                "name": name,
                "path": entry.path,
                "status": "kept",
            })),
            None,
        );

        let updated = self.index.find(name)?.unwrap_or(entry);
        serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
    }

    /// List all tracked worktrees.
    pub fn list_all(&self) -> Result<String, String> {
        self.index.list_all()
    }

    /// Run a command in a worktree directory.
    pub fn run(&self, name: &str, command: &str) -> Result<String, String> {
        let blocked = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
        if blocked.iter().any(|b| command.contains(b)) {
            return Err("Dangerous command blocked".to_string());
        }

        let entry = self
            .index
            .find(name)?
            .ok_or_else(|| format!("Unknown worktree '{name}'"))?;
        let path = Path::new(&entry.path);
        if !path.exists() {
            return Err(format!("Worktree path missing: {}", path.display()));
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(path)
            .output()
            .map_err(|e| format!("Failed to run command: {e}"))?;

        let out = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let out = out.trim().to_string();
        if out.is_empty() {
            Ok("(no output)".to_string())
        } else if out.len() > 50000 {
            Ok(out[..50000].to_string())
        } else {
            Ok(out)
        }
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn index_ref(&self) -> &WorktreeIndex {
        &self.index
    }

    /// Get the worktrees directory path.
    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- Name validation tests --

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name("my-feature").is_ok());
        assert!(validate_name("auth_refactor").is_ok());
        assert!(validate_name("fix.123").is_ok());
        assert!(validate_name("a").is_ok());
        assert!(validate_name("a-b_c.d").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(validate_name("").is_err());
        assert!(validate_name("has spaces").is_err());
        assert!(validate_name("has/slash").is_err());
        assert!(validate_name(&"a".repeat(41)).is_err());
        assert!(validate_name("has@special").is_err());
    }

    // -- EventBus tests --

    #[test]
    fn test_event_bus_creation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.jsonl");
        let bus = EventBus::new(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_event_bus_emit_and_list() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.jsonl");
        let bus = EventBus::new(&path).unwrap();

        bus.emit(
            "worktree.create.before",
            Some(serde_json::json!({"id": 1})),
            Some(serde_json::json!({"name": "test"})),
            None,
        );
        bus.emit(
            "worktree.create.after",
            Some(serde_json::json!({"id": 1})),
            Some(serde_json::json!({"name": "test", "status": "active"})),
            None,
        );

        let result = bus.list_recent(10).unwrap();
        assert!(result.contains("worktree.create.before"));
        assert!(result.contains("worktree.create.after"));
    }

    #[test]
    fn test_event_bus_limit() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.jsonl");
        let bus = EventBus::new(&path).unwrap();

        for i in 0..10 {
            bus.emit(&format!("event_{i}"), None, None, None);
        }

        let result = bus.list_recent(3).unwrap();
        let events: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(events.len(), 3);
    }

    // -- WorktreeIndex tests --

    #[test]
    fn test_worktree_index_creation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.json");
        let index = WorktreeIndex::new(&path).unwrap();
        assert!(path.exists());
        let list = index.list_all().unwrap();
        assert!(list.contains("No worktrees"));
    }

    #[test]
    fn test_worktree_index_add_and_find() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.json");
        let index = WorktreeIndex::new(&path).unwrap();

        let entry = WorktreeEntry {
            name: "feature-a".to_string(),
            path: "/tmp/feature-a".to_string(),
            branch: "wt/feature-a".to_string(),
            task_id: Some(1),
            status: "active".to_string(),
            created_at: None,
            removed_at: None,
            kept_at: None,
        };

        index.add(&entry).unwrap();

        let found = index.find("feature-a").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().branch, "wt/feature-a");

        assert!(index.find("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_worktree_index_duplicate_rejected() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.json");
        let index = WorktreeIndex::new(&path).unwrap();

        let entry = WorktreeEntry {
            name: "dup".to_string(),
            path: "/tmp/dup".to_string(),
            branch: "wt/dup".to_string(),
            task_id: None,
            status: "active".to_string(),
            created_at: None,
            removed_at: None,
            kept_at: None,
        };

        index.add(&entry).unwrap();
        let result = index.add(&entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_worktree_index_update_status() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.json");
        let index = WorktreeIndex::new(&path).unwrap();

        let entry = WorktreeEntry {
            name: "test".to_string(),
            path: "/tmp/test".to_string(),
            branch: "wt/test".to_string(),
            task_id: None,
            status: "active".to_string(),
            created_at: None,
            removed_at: None,
            kept_at: None,
        };
        index.add(&entry).unwrap();

        index.update_status("test", "removed").unwrap();
        let found = index.find("test").unwrap().unwrap();
        assert_eq!(found.status, "removed");
        assert!(found.removed_at.is_some());
    }

    #[test]
    fn test_worktree_index_list_all() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.json");
        let index = WorktreeIndex::new(&path).unwrap();

        index
            .add(&WorktreeEntry {
                name: "a".to_string(),
                path: "/tmp/a".to_string(),
                branch: "wt/a".to_string(),
                task_id: Some(1),
                status: "active".to_string(),
                created_at: None,
                removed_at: None,
                kept_at: None,
            })
            .unwrap();

        index
            .add(&WorktreeEntry {
                name: "b".to_string(),
                path: "/tmp/b".to_string(),
                branch: "wt/b".to_string(),
                task_id: None,
                status: "kept".to_string(),
                created_at: None,
                removed_at: None,
                kept_at: None,
            })
            .unwrap();

        let list = index.list_all().unwrap();
        assert!(list.contains("[active] a"));
        assert!(list.contains("[kept] b"));
        assert!(list.contains("task=1"));
    }

    // -- TaskBinding tests --

    #[test]
    fn test_task_binding_bind() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let task = serde_json::json!({
            "id": 1, "subject": "Test", "status": "pending", "owner": "", "worktree": ""
        });
        fs::write(
            tasks_dir.join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let binding = TaskBinding::new(&tasks_dir).unwrap();
        binding.bind(1, "feature-a").unwrap();

        let content = fs::read_to_string(tasks_dir.join("task_1.json")).unwrap();
        let updated: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(updated["worktree"], "feature-a");
        assert_eq!(updated["status"], "in_progress");
    }

    #[test]
    fn test_task_binding_unbind() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let task = serde_json::json!({
            "id": 1, "subject": "Test", "status": "in_progress", "owner": "alice", "worktree": "wt-a"
        });
        fs::write(
            tasks_dir.join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let binding = TaskBinding::new(&tasks_dir).unwrap();
        binding.unbind(1).unwrap();

        let content = fs::read_to_string(tasks_dir.join("task_1.json")).unwrap();
        let updated: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(updated["worktree"], "");
    }

    #[test]
    fn test_task_binding_complete() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let task = serde_json::json!({
            "id": 1, "subject": "Test", "status": "in_progress", "owner": "alice", "worktree": "wt-a"
        });
        fs::write(
            tasks_dir.join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let binding = TaskBinding::new(&tasks_dir).unwrap();
        binding.complete(1).unwrap();

        let content = fs::read_to_string(tasks_dir.join("task_1.json")).unwrap();
        let updated: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(updated["status"], "completed");
        assert_eq!(updated["worktree"], "");
    }

    #[test]
    fn test_task_binding_not_found() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        let binding = TaskBinding::new(&tasks_dir).unwrap();

        assert!(binding.bind(999, "test").is_err());
        assert!(binding.unbind(999).is_err());
        assert!(binding.complete(999).is_err());
    }

    // -- WorktreeManager tests --

    #[test]
    fn test_worktree_manager_creation() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorktreeManager::new(tmp.path()).unwrap();
        let list = mgr.list_all().unwrap();
        assert!(list.contains("No worktrees"));
    }

    #[test]
    fn test_worktree_manager_no_git() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorktreeManager::new(tmp.path()).unwrap();
        // Not a git repo, should fail
        let result = mgr.create("test", None, "HEAD");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not in a git repository"));
    }

    // -- Serialization tests --

    #[test]
    fn test_worktree_entry_serialization() {
        let entry = WorktreeEntry {
            name: "feature".to_string(),
            path: "/tmp/feature".to_string(),
            branch: "wt/feature".to_string(),
            task_id: Some(42),
            status: "active".to_string(),
            created_at: Some(1234567890.0),
            removed_at: None,
            kept_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: WorktreeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "feature");
        assert_eq!(parsed.task_id, Some(42));
        // removed_at is None, should not appear in JSON
        assert!(!json.contains("removed_at"));
    }

    #[test]
    fn test_worktree_event_serialization() {
        let event = WorktreeEvent {
            event: "worktree.create".to_string(),
            ts: 1234567890.0,
            task: serde_json::json!({"id": 1}),
            worktree: serde_json::json!({"name": "test"}),
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("worktree.create"));
        assert!(!json.contains("error")); // None should be skipped
    }

    #[test]
    fn test_detect_repo_root_in_repo() {
        let cwd = std::env::current_dir().unwrap();
        let root = detect_repo_root(&cwd);
        // We're in a git repo, so it should find the root
        assert!(root.is_some());
    }

    #[test]
    fn test_detect_repo_root_not_in_repo() {
        let tmp = TempDir::new().unwrap();
        let root = detect_repo_root(tmp.path());
        assert!(root.is_none());
    }
}
