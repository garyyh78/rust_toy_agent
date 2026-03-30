//! background_tasks.rs - Background task execution with notification queue
//!
//! Run commands in background threads. A notification queue is drained
//! before each LLM call to deliver results.
//!
//! Key insight: "Fire and forget -- the agent doesn't block while the command runs."

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub task_id: String,
    pub status: String,
    pub command: String,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub task_id: String,
    pub status: String,
    pub command: String,
    pub result: String,
}

pub struct BackgroundManager {
    tasks: Arc<Mutex<HashMap<String, BackgroundTask>>>,
    notification_queue: Arc<Mutex<Vec<Notification>>>,
}

impl BackgroundManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            notification_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn run(&self, command: &str, workdir: &std::path::Path) -> String {
        let task_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let task_id_for_thread = task_id.clone();
        let command_owned = command.to_string();
        let command_for_output = command_owned.clone();
        let command_for_status = command_owned.clone();
        let tasks = Arc::clone(&self.tasks);
        let notification_queue = Arc::clone(&self.notification_queue);
        let workdir = workdir.to_path_buf();

        {
            let mut tasks_lock = tasks.lock().unwrap();
            tasks_lock.insert(
                task_id.clone(),
                BackgroundTask {
                    task_id: task_id.clone(),
                    status: "running".to_string(),
                    command: command.to_string(),
                    result: None,
                },
            );
        }

        thread::spawn(move || {
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(command_for_status)
                .current_dir(&workdir)
                .output()
                .map(|o| {
                    if o.status.success() {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    }
                })
                .unwrap_or_else(|_| "error".to_string());

            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(command_for_output)
                .current_dir(&workdir)
                .output()
                .map(|o| {
                    let out = o.stdout;
                    let err = o.stderr;
                    let combined = [out, err].concat();
                    String::from_utf8_lossy(&combined).trim().to_string()
                })
                .unwrap_or_else(|e| format!("Error: {}", e));

            let output_truncated = if output.len() > 50000 {
                output[..50000].to_string()
            } else {
                output.clone()
            };

            {
                let mut tasks_lock = tasks.lock().unwrap();
                if let Some(task) = tasks_lock.get_mut(&task_id_for_thread) {
                    task.status = status.clone();
                    task.result = Some(output_truncated.clone());
                }
            }

            {
                let mut queue = notification_queue.lock().unwrap();
                queue.push(Notification {
                    task_id: task_id_for_thread,
                    status,
                    command: command_owned[..command_owned.len().min(80)].to_string(),
                    result: if output_truncated.len() > 500 {
                        output_truncated[..500].to_string()
                    } else {
                        output_truncated
                    },
                });
            }
        });

        format!(
            "Background task {} started: {}",
            task_id,
            &command[..command.len().min(80)]
        )
    }

    pub fn check(&self, task_id: Option<&str>) -> String {
        let tasks = self.tasks.lock().unwrap();

        if let Some(tid) = task_id {
            if let Some(task) = tasks.get(tid) {
                let result = task.result.as_deref().unwrap_or("(running)");
                return format!(
                    "[{}] {}\n{}",
                    task.status,
                    &task.command[..task.command.len().min(60)],
                    result
                );
            } else {
                return format!("Error: Unknown task {}", tid);
            }
        }

        if tasks.is_empty() {
            return "No background tasks.".to_string();
        }

        let lines: Vec<String> = tasks
            .values()
            .map(|t| {
                format!(
                    "{}: [{}] {}",
                    t.task_id,
                    t.status,
                    &t.command[..t.command.len().min(60)]
                )
            })
            .collect();

        lines.join("\n")
    }

    pub fn drain_notifications(&self) -> Vec<Notification> {
        let mut queue = self.notification_queue.lock().unwrap();
        let notifs: Vec<Notification> = queue.drain(..).collect();
        notifs
    }
}

impl Default for BackgroundManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_background_task_creation() {
        let tmp = TempDir::new().unwrap();
        let mgr = BackgroundManager::new();

        let result = mgr.run("echo hello", tmp.path());
        assert!(result.contains("started"));

        thread::sleep(Duration::from_millis(100));

        let status = mgr.check(None);
        assert!(status.contains("completed") || status.contains("running"));
    }

    #[test]
    fn test_drain_notifications() {
        let tmp = TempDir::new().unwrap();
        let mgr = BackgroundManager::new();

        mgr.run("sleep 0.2", tmp.path());
        thread::sleep(Duration::from_millis(500));

        let notifs = mgr.drain_notifications();
        assert!(!notifs.is_empty());
    }
}
