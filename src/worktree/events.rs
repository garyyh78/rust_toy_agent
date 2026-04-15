use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

pub struct EventBus {
    path: PathBuf,
}

impl EventBus {
    pub fn new(event_log_path: &PathBuf) -> std::io::Result<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_event_bus_creation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.jsonl");
        let _bus = EventBus::new(&path).unwrap();
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
}
