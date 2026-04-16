use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub struct WorktreeIndex {
    path: PathBuf,
}

impl WorktreeIndex {
    /// Create a new worktree index.
    ///
    /// # Errors
    /// Returns an error if the index file cannot be created.
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

    /// Find a worktree by name.
    ///
    /// # Errors
    /// Returns an error if the index cannot be read or parsed.
    pub fn find(&self, name: &str) -> Result<Option<WorktreeEntry>, String> {
        let entries = self.load()?;
        Ok(entries.into_iter().find(|e| e.name == name))
    }

    /// Add a worktree entry to the index.
    ///
    /// # Errors
    /// Returns an error if adding fails (duplicate, read/write error).
    pub fn add(&self, entry: &WorktreeEntry) -> Result<(), String> {
        let mut entries = self.load()?;
        if entries.iter().any(|e| e.name == entry.name) {
            return Err(format!("Worktree '{}' already exists in index", entry.name));
        }
        entries.push(entry.clone());
        self.save(&entries)
    }

    /// Update the status of a worktree.
    ///
    /// # Errors
    /// Returns an error if the index cannot be read or written.
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

    /// List all worktrees in the index.
    ///
    /// # Errors
    /// Returns an error if the index cannot be read or parsed.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
