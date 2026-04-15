//! `background_tasks.rs` - Background task execution with notification queue
//!
//! Run commands in background threads. A notification queue is drained
//! before each LLM call to deliver results.
//!
//! Key insight: "Fire and forget -- the agent doesn't block while the command runs."

use crate::config::{BASH_ENV_ALLOWLIST, MAX_TOOL_OUTPUT_BYTES};
use crate::text_util::truncate_chars;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc;

fn build_command(command: &str, workdir: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd.current_dir(workdir);
    cmd.env_clear();
    for key in BASH_ENV_ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    cmd
}

/// Maximum length for notification result text.
const MAX_NOTIFICATION_SIZE: usize = 500;

/// Maximum length for command display in output.
const MAX_COMMAND_DISPLAY: usize = 80;

/// Maximum length for status check command display.
const MAX_STATUS_COMMAND_DISPLAY: usize = 60;

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
    tasks: Arc<DashMap<String, BackgroundTask>>,
    tx: mpsc::UnboundedSender<Notification>,
    rx: Arc<std::sync::Mutex<mpsc::UnboundedReceiver<Notification>>>,
}

impl BackgroundManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tasks: Arc::new(DashMap::new()),
            tx,
            rx: Arc::new(std::sync::Mutex::new(rx)),
        }
    }

    pub fn run(&self, command: &str, workdir: &std::path::Path) -> String {
        let task_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let task_id_for_thread = task_id.clone();
        let command_owned = command.to_string();
        let tasks = Arc::clone(&self.tasks);
        let tx = self.tx.clone();
        let workdir = workdir.to_path_buf();

        {
            tasks.insert(
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
            let output_result = build_command(&command_owned, &workdir).output();

            let (status, output) = match output_result {
                Ok(o) => {
                    let status = if o.status.success() {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    };
                    let combined = [o.stdout, o.stderr].concat();
                    let text = String::from_utf8_lossy(&combined).trim().to_string();
                    (status, text)
                }
                Err(e) => ("error".to_string(), format!("Error: {e}")),
            };

            let output_truncated = if output.len() > MAX_TOOL_OUTPUT_BYTES {
                truncate_chars(&output, MAX_TOOL_OUTPUT_BYTES)
            } else {
                output.clone()
            };

            if let Some(mut task) = tasks.get_mut(&task_id_for_thread) {
                task.status = status.clone();
                task.result = Some(output_truncated.clone());
            }

            let notif = Notification {
                task_id: task_id_for_thread,
                status,
                command: truncate_chars(&command_owned, MAX_COMMAND_DISPLAY),
                result: if output_truncated.len() > MAX_NOTIFICATION_SIZE {
                    truncate_chars(&output_truncated, MAX_NOTIFICATION_SIZE)
                } else {
                    output_truncated
                },
            };
            let _ = tx.send(notif);
        });

        format!(
            "Background task {} started: {}",
            task_id,
            &truncate_chars(command, MAX_COMMAND_DISPLAY)
        )
    }

    pub fn check(&self, task_id: Option<&str>) -> String {
        if let Some(tid) = task_id {
            if let Some(task) = self.tasks.get(tid) {
                let result = task.result.as_deref().unwrap_or("(running)");
                return format!(
                    "[{}] {}\n{}",
                    task.status,
                    &truncate_chars(&task.command, MAX_STATUS_COMMAND_DISPLAY),
                    result
                );
            }
            return format!("Error: Unknown task {tid}");
        }

        if self.tasks.is_empty() {
            return "No background tasks.".to_string();
        }

        let lines: Vec<String> = self
            .tasks
            .iter()
            .map(|t| {
                format!(
                    "{}: [{}] {}",
                    t.key(),
                    t.status,
                    truncate_chars(&t.command, MAX_STATUS_COMMAND_DISPLAY)
                )
            })
            .collect();

        lines.join("\n")
    }

    pub fn drain_notifications(&self) -> Vec<Notification> {
        let mut queue = match self.rx.lock() {
            Ok(q) => q,
            Err(e) => {
                tracing::error!(error = %e, "lock poisoned");
                return vec![];
            }
        };
        let mut notifs = Vec::new();
        while let Ok(notif) = queue.try_recv() {
            notifs.push(notif);
        }
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
    use std::time::Duration;
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

    #[test]
    fn background_command_runs_exactly_once() {
        let tmp = TempDir::new().unwrap();
        let mgr = BackgroundManager::new();
        let marker = tmp.path().join("marker.txt");

        mgr.run(&format!("echo x >> {}", marker.display()), tmp.path());
        thread::sleep(Duration::from_millis(500));

        let content = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(content, "x\n");
    }
}
