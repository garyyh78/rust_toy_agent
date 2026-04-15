#[cfg(test)]
mod tests {
    use crate::worktree::WorktreeManager;
    use crate::worktree::{WorktreeEntry, WorktreeEvent};
    use tempfile::TempDir;

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
        let result = mgr.create("test", None, "HEAD");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not in a git repository"));
    }

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
        assert!(!json.contains("error"));
    }
}
