use std::fs;
use std::path::PathBuf;

pub struct TaskBinding {
    tasks_dir: PathBuf,
}

impl TaskBinding {
    /// Create a new task binding manager.
    ///
    /// # Errors
    /// Returns an error if the directory cannot be created.
    pub fn new(tasks_dir: &PathBuf) -> std::io::Result<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
}
