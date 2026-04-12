//! task_system.rs - Persistent task management with dependency graph
//!
//! Tasks persist as JSON files in .tasks/ so they survive context compression.
//! Each task has a dependency graph (blockedBy/blocks).
//!
//! Key insight: "State that survives compression -- because it's outside the conversation."

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub subject: String,
    pub description: String,
    pub status: String,
    #[serde(rename = "blockedBy")]
    pub blocked_by: Vec<u32>,
    pub blocks: Vec<u32>,
    pub owner: String,
}

impl Task {
    pub fn new(id: u32, subject: &str, description: &str) -> Self {
        Self {
            id,
            subject: subject.to_string(),
            description: description.to_string(),
            status: "pending".to_string(),
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            owner: String::new(),
        }
    }
}

pub struct TaskManager {
    dir: std::path::PathBuf,
    next_id: u32,
}

impl TaskManager {
    pub fn new(tasks_dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(tasks_dir)?;
        let next_id = Self::max_id(tasks_dir) + 1;
        Ok(Self {
            dir: tasks_dir.to_path_buf(),
            next_id,
        })
    }

    fn max_id(tasks_dir: &Path) -> u32 {
        let mut max_id = 0u32;
        if let Ok(entries) = fs::read_dir(tasks_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("task_") && name.ends_with(".json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(task) = serde_json::from_str::<Task>(&content) {
                                if task.id > max_id {
                                    max_id = task.id;
                                }
                            }
                        }
                    }
                }
            }
        }
        max_id
    }

    fn load(&self, task_id: u32) -> Result<Task, String> {
        let path = self.dir.join(format!("task_{}.json", task_id));
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read task: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse task: {}", e))
    }

    fn save(&self, task: &Task) -> std::io::Result<()> {
        let path = self.dir.join(format!("task_{}.json", task.id));
        let content = serde_json::to_string_pretty(task)?;
        fs::write(path, content)
    }

    pub fn create(&mut self, subject: &str, description: &str) -> std::io::Result<String> {
        let task = Task::new(self.next_id, subject, description);
        self.save(&task)?;
        self.next_id += 1;
        Ok(serde_json::to_string_pretty(&task).unwrap_or_default())
    }

    pub fn get(&self, task_id: u32) -> Result<String, String> {
        let task = self.load(task_id)?;
        Ok(serde_json::to_string_pretty(&task).unwrap_or_default())
    }

    pub fn update(
        &mut self,
        task_id: u32,
        status: Option<&str>,
        add_blocked_by: Option<Vec<u32>>,
        add_blocks: Option<Vec<u32>>,
    ) -> Result<String, String> {
        let mut task = self.load(task_id)?;

        if let Some(s) = status {
            if !["pending", "in_progress", "completed"].contains(&s) {
                return Err(format!("Invalid status: {}", s));
            }
            task.status = s.to_string();
            if s == "completed" {
                self.clear_dependency(task_id);
            }
        }

        if let Some(blocked) = add_blocked_by {
            task.blocked_by.extend(blocked);
            task.blocked_by.sort();
            task.blocked_by.dedup();
        }

        if let Some(blocks) = add_blocks {
            task.blocks.extend(blocks.clone());
            task.blocks.sort();
            task.blocks.dedup();

            for blocked_id in &blocks {
                if let Ok(mut blocked) = self.load(*blocked_id) {
                    if !blocked.blocked_by.contains(&task_id) {
                        blocked.blocked_by.push(task_id);
                        let _ = self.save(&blocked);
                    }
                }
            }
        }

        self.save(&task).map_err(|e| e.to_string())?;
        Ok(serde_json::to_string_pretty(&task).unwrap_or_default())
    }

    fn clear_dependency(&self, completed_id: u32) {
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("task_") && name.ends_with(".json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(mut task) = serde_json::from_str::<Task>(&content) {
                                if task.blocked_by.contains(&completed_id) {
                                    task.blocked_by.retain(|&x| x != completed_id);
                                    let _ = self.save(&task);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn list_all(&self) -> String {
        let mut tasks: HashMap<u32, Task> = HashMap::new();

        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("task_") && name.ends_with(".json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(task) = serde_json::from_str::<Task>(&content) {
                                tasks.insert(task.id, task);
                            }
                        }
                    }
                }
            }
        }

        if tasks.is_empty() {
            return "No tasks.".to_string();
        }

        let mut lines = Vec::new();
        let mut task_vec: Vec<_> = tasks.values().collect();
        task_vec.sort_by_key(|t| &t.id);
        for task in task_vec {
            let marker = match task.status.as_str() {
                "pending" => "[ ]",
                "in_progress" => "[>]",
                "completed" => "[x]",
                _ => "[?]",
            };
            let blocked = if task.blocked_by.is_empty() {
                String::new()
            } else {
                format!(" (blocked by: {:?})", task.blocked_by)
            };
            lines.push(format!(
                "{} #{}: {}{}",
                marker, task.id, task.subject, blocked
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_task() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TaskManager::new(tmp.path()).unwrap();

        let result = mgr.create("Test task", "Description").unwrap();
        let task: Task = serde_json::from_str(&result).unwrap();

        assert_eq!(task.subject, "Test task");
        assert_eq!(task.status, "pending");
    }

    #[test]
    fn test_update_status() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TaskManager::new(tmp.path()).unwrap();

        mgr.create("Test task", "Description").unwrap();
        let result = mgr.update(1, Some("completed"), None, None).unwrap();
        let task: Task = serde_json::from_str(&result).unwrap();

        assert_eq!(task.status, "completed");
    }

    #[test]
    fn test_list_tasks() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TaskManager::new(tmp.path()).unwrap();

        mgr.create("Task 1", "").unwrap();
        mgr.create("Task 2", "").unwrap();

        let list = mgr.list_all();
        assert!(list.contains("Task 1"));
        assert!(list.contains("Task 2"));
    }
}
