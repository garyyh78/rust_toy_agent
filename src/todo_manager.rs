//! todo_manager.rs - Task tracking for the agent
//!
//! The LLM calls the "todo" tool to update this state.
//! Validation enforces: max 20 items, non-empty text,
//! one in_progress at a time, valid status enum.
//!
//! ┌────────────────────────────────────────────────────────────┐
//! │                     TodoManager                            │
//! ├────────────────────────────────────────────────────────────┤
//! │                                                            │
//! │  items: Vec<TodoItem>                                      │
//! │                                                            │
//! │  update(&[Json]) -> Result<String>                         │
//! │    ├── validate max 20 items                               │
//! │    ├── require non-empty text                              │
//! │    ├── one in_progress at a time                           │
//! │    └── valid status enum (pending/in_progress/completed)   │
//! │                                                            │
//! │  render() -> String                                        │
//! │    [ ] #1: pending task                                    │
//! │    [>] #2: in progress task                                │
//! │    [x] #3: completed task                                  │
//! │    (1/3 completed)                                         │
//! └────────────────────────────────────────────────────────────┘

use serde_json::Value as Json;

const MAX_TODO_ITEMS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: String,
}

pub struct TodoManager {
    items: Vec<TodoItem>,
}

impl Default for TodoManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoManager {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn items(&self) -> &[TodoItem] {
        &self.items
    }

    /// Validate and replace the full todo list. Returns rendered output on success.
    pub fn update(&mut self, items_json: &[Json]) -> Result<String, String> {
        if items_json.len() > MAX_TODO_ITEMS {
            return Err(format!("Max {MAX_TODO_ITEMS} todos allowed"));
        }
        let mut validated = Vec::new();
        let mut in_progress_count = 0usize;
        for (i, item) in items_json.iter().enumerate() {
            let text = item
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending")
                .to_lowercase();
            let default_id = format!("{}", i + 1);
            let item_id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(&default_id)
                .to_string();
            if text.is_empty() {
                return Err(format!("Item {item_id}: text required"));
            }
            if !matches!(status.as_str(), "pending" | "in_progress" | "completed") {
                return Err(format!("Item {item_id}: invalid status '{status}'"));
            }
            if status == "in_progress" {
                in_progress_count += 1;
            }
            validated.push(TodoItem {
                id: item_id,
                text,
                status,
            });
        }
        if in_progress_count > 1 {
            return Err("Only one task can be in_progress at a time".to_string());
        }
        self.items = validated;
        Ok(self.render())
    }

    /// Render the todo list as a human-readable string with status markers.
    pub fn render(&self) -> String {
        if self.items.is_empty() {
            return "No todos.".to_string();
        }
        let mut lines = Vec::new();
        for item in &self.items {
            let marker = match item.status.as_str() {
                "in_progress" => "[>]",
                "completed" => "[x]",
                _ => "[ ]",
            };
            lines.push(format!("{marker} #{}: {}", item.id, item.text));
        }
        let done = self
            .items
            .iter()
            .filter(|t| t.status == "completed")
            .count();
        lines.push(format!("\n({}/{} completed)", done, self.items.len()));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let mgr = TodoManager::new();
        assert_eq!(mgr.render(), "No todos.");
        assert_eq!(mgr.items().len(), 0);
    }

    #[test]
    fn test_default_is_empty() {
        let mgr = TodoManager::default();
        assert_eq!(mgr.render(), "No todos.");
    }

    #[test]
    fn test_basic_pending_and_in_progress() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Write tests", "status": "pending"}),
            serde_json::json!({"id": "2", "text": "Run build", "status": "in_progress"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[ ] #1: Write tests"));
        assert!(result.contains("[>] #2: Run build"));
        assert!(result.contains("(0/2 completed)"));
        assert_eq!(mgr.items().len(), 2);
    }

    #[test]
    fn test_completed_items() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Write tests", "status": "completed"}),
            serde_json::json!({"id": "2", "text": "Run build", "status": "completed"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[x] #1: Write tests"));
        assert!(result.contains("[x] #2: Run build"));
        assert!(result.contains("(2/2 completed)"));
    }

    #[test]
    fn test_mixed_statuses() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Done", "status": "completed"}),
            serde_json::json!({"id": "2", "text": "Working", "status": "in_progress"}),
            serde_json::json!({"id": "3", "text": "Waiting", "status": "pending"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[x] #1: Done"));
        assert!(result.contains("[>] #2: Working"));
        assert!(result.contains("[ ] #3: Waiting"));
        assert!(result.contains("(1/3 completed)"));
    }

    #[test]
    fn test_update_replaces_previous_items() {
        let mut mgr = TodoManager::new();
        mgr.update(&[serde_json::json!({"id": "1", "text": "Old", "status": "pending"})])
            .unwrap();
        assert_eq!(mgr.items().len(), 1);

        mgr.update(&[
            serde_json::json!({"id": "1", "text": "New A", "status": "completed"}),
            serde_json::json!({"id": "2", "text": "New B", "status": "pending"}),
        ])
        .unwrap();
        assert_eq!(mgr.items().len(), 2);
        assert!(mgr.render().contains("New A"));
        assert!(mgr.render().contains("New B"));
    }

    #[test]
    fn test_max_items_rejected() {
        let mut mgr = TodoManager::new();
        let items: Vec<Json> = (1..=21)
            .map(|i| {
                serde_json::json!({"id": format!("{i}"), "text": format!("task {i}"), "status": "pending"})
            })
            .collect();
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Max 20 todos"));
    }

    #[test]
    fn test_max_items_boundary_ok() {
        let mut mgr = TodoManager::new();
        let items: Vec<Json> = (1..=20)
            .map(|i| {
                serde_json::json!({"id": format!("{i}"), "text": format!("task {i}"), "status": "pending"})
            })
            .collect();
        assert!(mgr.update(&items).is_ok());
    }

    #[test]
    fn test_multiple_in_progress_rejected() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Task A", "status": "in_progress"}),
            serde_json::json!({"id": "2", "text": "Task B", "status": "in_progress"}),
        ];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Only one task can be in_progress"));
    }

    #[test]
    fn test_empty_text_rejected() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "", "status": "pending"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text required"));
    }

    #[test]
    fn test_whitespace_text_rejected() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "   ", "status": "pending"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text required"));
    }

    #[test]
    fn test_invalid_status_rejected() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "Task", "status": "done"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid status"));
    }

    #[test]
    fn test_missing_status_defaults_to_pending() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "Task"})];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[ ] #1: Task"));
    }

    #[test]
    fn test_missing_id_uses_index() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"text": "First", "status": "pending"}),
            serde_json::json!({"text": "Second", "status": "pending"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("#1: First"));
        assert!(result.contains("#2: Second"));
    }

    #[test]
    fn test_empty_list_clears_todos() {
        let mut mgr = TodoManager::new();
        mgr.update(&[serde_json::json!({"id": "1", "text": "Task", "status": "pending"})])
            .unwrap();
        assert_eq!(mgr.items().len(), 1);

        mgr.update(&[]).unwrap();
        assert_eq!(mgr.items().len(), 0);
        assert_eq!(mgr.render(), "No todos.");
    }

    #[test]
    fn test_render_format() {
        let mut mgr = TodoManager::new();
        mgr.update(&[
            serde_json::json!({"id": "1", "text": "Alpha", "status": "pending"}),
            serde_json::json!({"id": "2", "text": "Beta", "status": "in_progress"}),
            serde_json::json!({"id": "3", "text": "Gamma", "status": "completed"}),
        ])
        .unwrap();
        let rendered = mgr.render();
        assert!(rendered.contains("[ ] #1: Alpha"));
        assert!(rendered.contains("[>] #2: Beta"));
        assert!(rendered.contains("[x] #3: Gamma"));
        assert!(rendered.contains("(1/3 completed)"));
    }

    #[test]
    fn test_items_accessor() {
        let mut mgr = TodoManager::new();
        assert!(mgr.items().is_empty());

        mgr.update(&[serde_json::json!({"id": "1", "text": "Test", "status": "completed"})])
            .unwrap();
        assert_eq!(mgr.items().len(), 1);
        assert_eq!(mgr.items()[0].id, "1");
        assert_eq!(mgr.items()[0].text, "Test");
        assert_eq!(mgr.items()[0].status, "completed");
    }
}
