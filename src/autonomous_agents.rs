//! autonomous_agents.rs - Autonomous agents with idle polling and task claiming
//!
//! Idle cycle with task board polling, auto-claiming unclaimed tasks, and
//! identity re-injection after context compression.
//!
//! ```text
//!     Teammate lifecycle:
//!     +-------+
//!     | spawn |
//!     +---+---+
//!         |
//!         v
//!     +-------+  tool_use    +-------+
//!     | WORK  | <----------- |  LLM  |
//!     +---+---+              +-------+
//!         |
//!         | stop_reason != tool_use
//!         v
//!     +--------+
//!     | IDLE   | poll every 5s for up to 60s
//!     +---+----+
//!         |
//!         +---> check inbox -> message? -> resume WORK
//!         |
//!         +---> scan .tasks/ -> unclaimed? -> claim -> resume WORK
//!         |
//!         +---> timeout (60s) -> shutdown
//!
//!     Identity re-injection after compression:
//!     messages = [identity_block, ...remaining...]
//!     "You are 'coder', role: backend, team: my-team"
//!
//! Key insight: "The agent finds work itself."
//!
//! Uses `notify` crate for efficient filesystem watching of the tasks directory.

use notify::{recommended_watcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

/// Default polling interval in seconds.
pub const POLL_INTERVAL_SECS: u64 = 5;

/// Default idle timeout in seconds.
pub const IDLE_TIMEOUT_SECS: u64 = 60;

/// A task from the task board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default, rename = "blockedBy")]
    pub blocked_by: Vec<u32>,
}

fn default_status() -> String {
    "pending".to_string()
}

/// Identity block for re-injection after context compression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBlock {
    pub role: String,
    pub content: String,
}

/// Create an identity block for re-injection after compression.
pub fn make_identity_block(name: &str, role: &str, team_name: &str) -> IdentityBlock {
    IdentityBlock {
        role: "user".to_string(),
        content: format!(
            "<identity>You are '{name}', role: {role}, team: {team_name}. Continue your work.</identity>"
        ),
    }
}

/// Scan the tasks directory for unclaimed tasks.
pub fn scan_unclaimed_tasks(tasks_dir: &Path) -> Vec<Task> {
    let mut unclaimed = Vec::new();
    if !tasks_dir.exists() {
        return unclaimed;
    }
    let mut entries: Vec<_> = match fs::read_dir(tasks_dir) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return unclaimed,
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("task_") && name.ends_with(".json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(task) = serde_json::from_str::<Task>(&content) {
                        if task.status == "pending"
                            && task.owner.is_empty()
                            && task.blocked_by.is_empty()
                        {
                            unclaimed.push(task);
                        }
                    }
                }
            }
        }
    }
    unclaimed
}

/// Claim a task for an owner.
pub fn claim_task(tasks_dir: &Path, task_id: u32, owner: &str) -> Result<String, String> {
    let path = tasks_dir.join(format!("task_{task_id}.json"));
    if !path.exists() {
        return Err(format!("Task {task_id} not found"));
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut task: Task = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    task.owner = owner.to_string();
    task.status = "in_progress".to_string();
    let updated = serde_json::to_string_pretty(&task).map_err(|e| e.to_string())?;
    fs::write(&path, updated).map_err(|e| e.to_string())?;
    Ok(format!("Claimed task #{task_id} for {owner}"))
}

/// Watch the tasks directory for new task files using notify.
/// Returns a watcher handle and a receiver for filesystem events.
/// The watcher is dropped when the returned value is dropped.
pub fn watch_tasks_dir(
    tasks_dir: &Path,
) -> Result<
    (
        notify::RecommendedWatcher,
        mpsc::Receiver<notify::Result<notify::Event>>,
    ),
    String,
> {
    fs::create_dir_all(tasks_dir).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| format!("Failed to create watcher: {e}"))?;
    watcher
        .watch(tasks_dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch directory: {e}"))?;
    Ok((watcher, rx))
}

/// Poll for new work (inbox messages or unclaimed tasks).
/// Returns true if work was found, false on timeout.
pub fn poll_for_work(
    tasks_dir: &Path,
    inbox_check: &dyn Fn() -> Vec<serde_json::Value>,
    poll_interval: Duration,
    timeout: Duration,
) -> PollResult {
    let start = SystemTime::now();
    loop {
        // Check inbox
        let inbox = inbox_check();
        if !inbox.is_empty() {
            return PollResult::InboxMessages(inbox);
        }

        // Check for unclaimed tasks
        let unclaimed = scan_unclaimed_tasks(tasks_dir);
        if !unclaimed.is_empty() {
            return PollResult::UnclaimedTask(unclaimed[0].clone());
        }

        // Check timeout
        if start.elapsed().unwrap_or(Duration::MAX) >= timeout {
            return PollResult::Timeout;
        }

        std::thread::sleep(poll_interval);
    }
}

/// Result of polling for new work.
#[derive(Debug)]
pub enum PollResult {
    /// Inbox has new messages.
    InboxMessages(Vec<serde_json::Value>),
    /// Found an unclaimed task.
    UnclaimedTask(Task),
    /// Timed out waiting for work.
    Timeout,
}

/// Status manager for teammate lifecycle.
#[derive(Debug, Clone)]
pub struct StatusManager {
    statuses: HashMap<String, String>,
}

impl StatusManager {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: &str, status: &str) {
        self.statuses.insert(name.to_string(), status.to_string());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.statuses.get(name).map(|s| s.as_str())
    }

    pub fn remove(&mut self, name: &str) {
        self.statuses.remove(name);
    }
}

impl Default for StatusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- Identity block tests --

    #[test]
    fn test_make_identity_block() {
        let block = make_identity_block("coder", "backend", "my-team");
        assert_eq!(block.role, "user");
        assert!(block.content.contains("coder"));
        assert!(block.content.contains("backend"));
        assert!(block.content.contains("my-team"));
        assert!(block.content.contains("<identity>"));
    }

    #[test]
    fn test_identity_block_serialization() {
        let block = make_identity_block("alice", "frontend", "team-a");
        let json = serde_json::to_string(&block).unwrap();
        let parsed: IdentityBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "user");
        assert!(parsed.content.contains("alice"));
    }

    // -- Task scanning tests --

    #[test]
    fn test_scan_unclaimed_tasks_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let unclaimed = scan_unclaimed_tasks(tmp.path());
        assert!(unclaimed.is_empty());
    }

    #[test]
    fn test_scan_unclaimed_tasks_pending() {
        let tmp = TempDir::new().unwrap();
        let task = serde_json::json!({
            "id": 1,
            "subject": "Implement feature",
            "description": "Add login",
            "status": "pending",
            "owner": "",
            "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let unclaimed = scan_unclaimed_tasks(tmp.path());
        assert_eq!(unclaimed.len(), 1);
        assert_eq!(unclaimed[0].subject, "Implement feature");
    }

    #[test]
    fn test_scan_unclaimed_tasks_skips_claimed() {
        let tmp = TempDir::new().unwrap();

        // Pending, no owner -> should be found
        let task1 = serde_json::json!({
            "id": 1, "subject": "Task 1", "status": "pending", "owner": "", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_1.json"),
            serde_json::to_string_pretty(&task1).unwrap(),
        )
        .unwrap();

        // In progress with owner -> should be skipped
        let task2 = serde_json::json!({
            "id": 2, "subject": "Task 2", "status": "in_progress", "owner": "alice", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_2.json"),
            serde_json::to_string_pretty(&task2).unwrap(),
        )
        .unwrap();

        // Pending with owner -> should be skipped
        let task3 = serde_json::json!({
            "id": 3, "subject": "Task 3", "status": "pending", "owner": "bob", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_3.json"),
            serde_json::to_string_pretty(&task3).unwrap(),
        )
        .unwrap();

        // Pending but blocked -> should be skipped
        let task4 = serde_json::json!({
            "id": 4, "subject": "Task 4", "status": "pending", "owner": "", "blockedBy": [1]
        });
        fs::write(
            tmp.path().join("task_4.json"),
            serde_json::to_string_pretty(&task4).unwrap(),
        )
        .unwrap();

        let unclaimed = scan_unclaimed_tasks(tmp.path());
        assert_eq!(unclaimed.len(), 1);
        assert_eq!(unclaimed[0].id, 1);
    }

    #[test]
    fn test_scan_unclaimed_tasks_skips_completed() {
        let tmp = TempDir::new().unwrap();
        let task = serde_json::json!({
            "id": 1, "subject": "Done", "status": "completed", "owner": "alice", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let unclaimed = scan_unclaimed_tasks(tmp.path());
        assert!(unclaimed.is_empty());
    }

    // -- Claim task tests --

    #[test]
    fn test_claim_task_success() {
        let tmp = TempDir::new().unwrap();
        let task = serde_json::json!({
            "id": 1, "subject": "Test", "status": "pending", "owner": "", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let result = claim_task(tmp.path(), 1, "alice").unwrap();
        assert!(result.contains("Claimed"));
        assert!(result.contains("alice"));

        // Verify file was updated
        let content = fs::read_to_string(tmp.path().join("task_1.json")).unwrap();
        let updated: Task = serde_json::from_str(&content).unwrap();
        assert_eq!(updated.owner, "alice");
        assert_eq!(updated.status, "in_progress");
    }

    #[test]
    fn test_claim_task_not_found() {
        let tmp = TempDir::new().unwrap();
        let result = claim_task(tmp.path(), 999, "alice");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -- Poll for work tests --

    #[test]
    fn test_poll_inbox_messages() {
        let tmp = TempDir::new().unwrap();
        let msg = serde_json::json!({"type": "message", "from": "lead", "content": "hello"});
        let result = poll_for_work(
            tmp.path(),
            &|| vec![msg.clone()],
            Duration::from_millis(10),
            Duration::from_millis(100),
        );
        match result {
            PollResult::InboxMessages(msgs) => assert_eq!(msgs.len(), 1),
            _ => panic!("Expected InboxMessages"),
        }
    }

    #[test]
    fn test_poll_unclaimed_task() {
        let tmp = TempDir::new().unwrap();
        let task = serde_json::json!({
            "id": 1, "subject": "Work", "status": "pending", "owner": "", "blockedBy": []
        });
        fs::write(
            tmp.path().join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        let result = poll_for_work(
            tmp.path(),
            &|| vec![],
            Duration::from_millis(10),
            Duration::from_millis(100),
        );
        match result {
            PollResult::UnclaimedTask(t) => assert_eq!(t.subject, "Work"),
            _ => panic!("Expected UnclaimedTask"),
        }
    }

    #[test]
    fn test_poll_timeout() {
        let tmp = TempDir::new().unwrap();
        let result = poll_for_work(
            tmp.path(),
            &|| vec![],
            Duration::from_millis(10),
            Duration::from_millis(50),
        );
        assert!(matches!(result, PollResult::Timeout));
    }

    // -- Watcher tests --

    #[test]
    fn test_watch_tasks_dir_creation() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        let result = watch_tasks_dir(&tasks_dir);
        assert!(result.is_ok());
        assert!(tasks_dir.exists());
    }

    #[test]
    fn test_watch_tasks_dir_detects_new_file() {
        let tmp = TempDir::new().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        let (_watcher, rx) = watch_tasks_dir(&tasks_dir).unwrap();

        // Write a new task file
        let task = serde_json::json!({
            "id": 1, "subject": "New task", "status": "pending", "owner": ""
        });
        fs::write(
            tasks_dir.join("task_1.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();

        // Wait for the event
        let event = rx.recv_timeout(Duration::from_secs(2));
        assert!(event.is_ok(), "Should receive a filesystem event");
    }

    // -- Status manager tests --

    #[test]
    fn test_status_manager() {
        let mut sm = StatusManager::new();
        assert!(sm.get("alice").is_none());

        sm.set("alice", "working");
        assert_eq!(sm.get("alice"), Some("working"));

        sm.set("alice", "idle");
        assert_eq!(sm.get("alice"), Some("idle"));

        sm.remove("alice");
        assert!(sm.get("alice").is_none());
    }

    // -- Task serialization tests --

    #[test]
    fn test_task_serialization() {
        let task = Task {
            id: 1,
            subject: "Test task".to_string(),
            description: "A test".to_string(),
            status: "pending".to_string(),
            owner: String::new(),
            blocked_by: vec![],
        };
        let json = serde_json::to_string_pretty(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 1);
        assert_eq!(parsed.subject, "Test task");
        assert_eq!(parsed.status, "pending");
    }

    #[test]
    fn test_task_default_status() {
        let json = r#"{"id": 1, "subject": "Test"}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.status, "pending");
        assert!(task.owner.is_empty());
        assert!(task.blocked_by.is_empty());
    }
}
