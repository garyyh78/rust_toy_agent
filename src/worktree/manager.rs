use crate::worktree::binding::TaskBinding;
use crate::worktree::events::EventBus;
use crate::worktree::git;
use crate::worktree::index::{WorktreeEntry, WorktreeIndex};
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn detect_repo_root(cwd: &Path) -> Option<PathBuf> {
    let repo = Repository::discover(cwd).ok()?;
    repo.workdir().map(|p| p.to_path_buf())
}

/// Validates a worktree name.
///
/// # Errors
/// Returns an error if the name is empty, longer than 40 characters,
/// or contains invalid characters.
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

#[cfg(test)]
mod validate_name_tests {
    use super::validate_name;

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
}

#[cfg(test)]
mod detect_repo_root_tests {
    use super::detect_repo_root;
    use tempfile::TempDir;

    #[test]
    fn test_detect_repo_root_in_repo() {
        let cwd = std::env::current_dir().unwrap();
        let root = detect_repo_root(&cwd);
        assert!(root.is_some());
    }

    #[test]
    fn test_detect_repo_root_not_in_repo() {
        let tmp = TempDir::new().unwrap();
        let root = detect_repo_root(tmp.path());
        assert!(root.is_none());
    }
}

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

    pub fn status(&self, name: &str) -> Result<String, String> {
        let entry = self
            .index
            .find(name)?
            .ok_or_else(|| format!("Unknown worktree '{name}'"))?;
        let path = Path::new(&entry.path);
        if !path.exists() {
            return Err(format!("Worktree path missing: {}", path.display()));
        }
        git::get_worktree_status(path)
    }

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

    pub fn list_all(&self) -> Result<String, String> {
        self.index.list_all()
    }

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

    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }
}
