//! background_tasks.rs - Background task execution with notification queue
//!
//! Run commands in background threads. A notification queue is drained
//! before each LLM call to deliver results.
//!
//! Key insight: "Fire and forget -- the agent doesn't block while the command runs."

use crate::text_util::truncate_chars;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

/// Maximum output size for background task results (50KB).
const MAX_BG_OUTPUT_SIZE: usize = 50_000;

const BASH_ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "TMPDIR", "SHELL", "PWD",
];

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
            let mut tasks_lock = match tasks.lock() {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(error = %e, "lock poisoned");
                    return "Error: lock poisoned".to_string();
                }
            };
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
            let status = build_command(&command_for_status, &workdir)
                .output()
                .map(|o| {
                    if o.status.success() {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    }
                })
                .unwrap_or_else(|_| "error".to_string());

            let output = build_command(&command_for_output, &workdir)
                .output()
                .map(|o| {
                    let out = o.stdout;
                    let err = o.stderr;
                    let combined = [out, err].concat();
                    String::from_utf8_lossy(&combined).trim().to_string()
                })
                .unwrap_or_else(|e| format!("Error: {}", e));

            let output_truncated = if output.len() > MAX_BG_OUTPUT_SIZE {
                truncate_chars(&output, MAX_BG_OUTPUT_SIZE)
            } else {
                output.clone()
            };

            {
                let mut tasks_lock = match tasks.lock() {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "lock poisoned");
                        return;
                    }
                };
                if let Some(task) = tasks_lock.get_mut(&task_id_for_thread) {
                    task.status = status.clone();
                    task.result = Some(output_truncated.clone());
                }
            }

            {
                let mut queue = match notification_queue.lock() {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "lock poisoned");
                        return;
                    }
                };
                queue.push(Notification {
                    task_id: task_id_for_thread,
                    status,
                    command: truncate_chars(&command_owned, MAX_COMMAND_DISPLAY),
                    result: if output_truncated.len() > MAX_NOTIFICATION_SIZE {
                        truncate_chars(&output_truncated, MAX_NOTIFICATION_SIZE)
                    } else {
                        output_truncated
                    },
                });
            }
        });

        format!(
            "Background task {} started: {}",
            task_id,
            &truncate_chars(command, MAX_COMMAND_DISPLAY)
        )
    }

    pub fn check(&self, task_id: Option<&str>) -> String {
        let tasks = match self.tasks.lock() {
            Ok(t) => t,
            Err(e) => return format!("Error: lock poisoned: {}", e),
        };

        if let Some(tid) = task_id {
            if let Some(task) = tasks.get(tid) {
                let result = task.result.as_deref().unwrap_or("(running)");
                return format!(
                    "[{}] {}\n{}",
                    task.status,
                    &truncate_chars(&task.command, MAX_STATUS_COMMAND_DISPLAY),
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
                    truncate_chars(&t.command, MAX_STATUS_COMMAND_DISPLAY)
                )
            })
            .collect();

        lines.join("\n")
    }

    pub fn drain_notifications(&self) -> Vec<Notification> {
        let mut queue = match self.notification_queue.lock() {
            Ok(q) => q,
            Err(e) => {
                tracing::error!(error = %e, "lock poisoned");
                return vec![];
            }
        };
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
}
