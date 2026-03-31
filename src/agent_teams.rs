//! agent_teams.rs - Agent Teams with persistent named teammates
//!
//! Persistent named agents with per-member inboxes. Each teammate runs
//! its own agent loop in a separate thread. Communication via crossbeam channels.
//!
//! ```text
//!     TeammateManager
//!     ├── config: .team/config.json
//!     │   {"team_name": "default",
//!     │    "members": [
//!     │      {"name":"alice", "role":"coder", "status":"idle"}
//!     │    ]}
//!     │
//!     ├── MessageBus (crossbeam-channel bounded MPMC)
//!     │   ├── send(sender, to, content, msg_type)  -> "Sent message to alice"
//!     │   ├── read_inbox(name)                      -> Vec<Message>
//!     │   └── broadcast(sender, content, teammates)  -> "Broadcast to N"
//!     │
//!     └── TeammateManager
//!         ├── spawn(name, role, prompt)  -> "Spawned 'alice' (role: coder)"
//!         ├── list_all()                 -> "Team: default\n  alice (coder): working"
//!         └── member_names()             -> ["alice"]
//! ```
//!
//! Key insight: "Teammates that can talk to each other."

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Valid message types in the team system.
pub const VALID_MSG_TYPES: &[&str] = &[
    "message",
    "broadcast",
    "shutdown_request",
    "shutdown_response",
    "plan_approval_response",
];

/// A message sent between teammates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub from: String,
    pub content: String,
    pub timestamp: f64,
}

impl Message {
    pub fn new(sender: &str, content: &str, msg_type: &str) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        Self {
            msg_type: msg_type.to_string(),
            from: sender.to_string(),
            content: content.to_string(),
            timestamp: ts,
        }
    }
}

/// MessageBus: thread-safe inbox per teammate.
///
/// Uses Arc<RwLock<HashMap>> for shared mutable state across threads.
/// In production, crossbeam-channel bounded MPMC channels could be used
/// for non-blocking send/receive, but the shared map approach is simpler
/// and sufficient for this implementation.
pub struct MessageBus {
    /// Accumulated inbox per member name.
    inbox: Arc<RwLock<HashMap<String, Vec<Message>>>>,
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Send a message to a teammate's inbox.
    pub fn send(
        &self,
        sender: &str,
        to: &str,
        content: &str,
        msg_type: &str,
    ) -> Result<String, String> {
        if !VALID_MSG_TYPES.contains(&msg_type) {
            return Err(format!(
                "Invalid type '{}'. Valid: {:?}",
                msg_type, VALID_MSG_TYPES
            ));
        }
        let msg = Message::new(sender, content, msg_type);
        let mut inbox = self.inbox.write().unwrap();
        inbox.entry(to.to_string()).or_default().push(msg);
        Ok(format!("Sent {} to {}", msg_type, to))
    }

    /// Read and drain a teammate's inbox.
    pub fn read_inbox(&self, name: &str) -> Vec<Message> {
        let mut inbox = self.inbox.write().unwrap();
        inbox.remove(name).unwrap_or_default()
    }

    /// Send a broadcast message to all teammates except the sender.
    pub fn broadcast(
        &self,
        sender: &str,
        content: &str,
        teammates: &[String],
    ) -> Result<String, String> {
        let mut count = 0usize;
        for name in teammates {
            if name != sender {
                self.send(sender, name, content, "broadcast")?;
                count += 1;
            }
        }
        Ok(format!("Broadcast to {count} teammates"))
    }
}

/// A team member's configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub role: String,
    pub status: String,
}

/// Team configuration persisted in .team/config.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    #[serde(default = "default_team_name")]
    pub team_name: String,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

fn default_team_name() -> String {
    "default".to_string()
}

/// TeammateManager: persistent named agents with file-based config.
pub struct TeammateManager {
    dir: PathBuf,
    config: TeamConfig,
}

impl TeammateManager {
    pub fn new(team_dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(team_dir)?;
        let config = Self::load_config(team_dir);
        Ok(Self {
            dir: team_dir.to_path_buf(),
            config,
        })
    }

    fn config_path(dir: &Path) -> PathBuf {
        dir.join("config.json")
    }

    fn load_config(dir: &Path) -> TeamConfig {
        let path = Self::config_path(dir);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<TeamConfig>(&content) {
                    return cfg;
                }
            }
        }
        TeamConfig {
            team_name: "default".to_string(),
            members: Vec::new(),
        }
    }

    fn save_config(&self) -> std::io::Result<()> {
        let path = Self::config_path(&self.dir);
        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(path, content)
    }

    /// Add or update a teammate in the config.
    pub fn spawn(&mut self, name: &str, role: &str) -> Result<String, String> {
        if let Some(member) = self.config.members.iter_mut().find(|m| m.name == name) {
            if !["idle", "shutdown"].contains(&member.status.as_str()) {
                return Err(format!("'{}' is currently {}", name, member.status));
            }
            member.status = "working".to_string();
            member.role = role.to_string();
        } else {
            self.config.members.push(TeamMember {
                name: name.to_string(),
                role: role.to_string(),
                status: "working".to_string(),
            });
        }
        self.save_config().map_err(|e| e.to_string())?;
        Ok(format!("Spawned '{name}' (role: {role})"))
    }

    /// List all teammates with name, role, status.
    pub fn list_all(&self) -> String {
        if self.config.members.is_empty() {
            return "No teammates.".to_string();
        }
        let mut lines = vec![format!("Team: {}", self.config.team_name)];
        for m in &self.config.members {
            lines.push(format!("  {} ({}): {}", m.name, m.role, m.status));
        }
        lines.join("\n")
    }

    /// Get all member names.
    pub fn member_names(&self) -> Vec<String> {
        self.config.members.iter().map(|m| m.name.clone()).collect()
    }

    /// Set a member's status.
    pub fn set_status(&mut self, name: &str, status: &str) {
        if let Some(member) = self.config.members.iter_mut().find(|m| m.name == name) {
            member.status = status.to_string();
            let _ = self.save_config();
        }
    }

    /// Get the team name.
    pub fn team_name(&self) -> &str {
        &self.config.team_name
    }

    /// Find a member by name.
    pub fn find_member(&self, name: &str) -> Option<&TeamMember> {
        self.config.members.iter().find(|m| m.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- Message tests --

    #[test]
    fn test_message_creation() {
        let msg = Message::new("alice", "hello", "message");
        assert_eq!(msg.from, "alice");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.msg_type, "message");
        assert!(msg.timestamp > 0.0);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::new("bob", "hi", "broadcast");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from, "bob");
        assert_eq!(parsed.msg_type, "broadcast");
    }

    // -- MessageBus tests --

    #[test]
    fn test_message_bus_send_and_read() {
        let bus = MessageBus::new();
        let result = bus.send("lead", "alice", "do task", "message");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("alice"));

        let msgs = bus.read_inbox("alice");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].from, "lead");
        assert_eq!(msgs[0].content, "do task");

        // Inbox is drained
        let empty = bus.read_inbox("alice");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_message_bus_invalid_type() {
        let bus = MessageBus::new();
        let result = bus.send("lead", "alice", "test", "invalid_type");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid type"));
    }

    #[test]
    fn test_message_bus_broadcast() {
        let bus = MessageBus::new();
        let teammates = vec!["alice".to_string(), "bob".to_string(), "lead".to_string()];
        let result = bus.broadcast("lead", "hello everyone", &teammates);
        assert!(result.unwrap().contains("Broadcast to 2"));

        let alice_msgs = bus.read_inbox("alice");
        assert_eq!(alice_msgs.len(), 1);
        assert_eq!(alice_msgs[0].msg_type, "broadcast");

        let bob_msgs = bus.read_inbox("bob");
        assert_eq!(bob_msgs.len(), 1);

        // Lead should not receive their own broadcast
        let lead_msgs = bus.read_inbox("lead");
        assert!(lead_msgs.is_empty());
    }

    #[test]
    fn test_message_bus_multiple_messages() {
        let bus = MessageBus::new();
        bus.send("lead", "alice", "msg1", "message").unwrap();
        bus.send("bob", "alice", "msg2", "message").unwrap();
        bus.send("lead", "bob", "msg3", "message").unwrap();

        let alice_msgs = bus.read_inbox("alice");
        assert_eq!(alice_msgs.len(), 2);
        assert_eq!(alice_msgs[0].content, "msg1");
        assert_eq!(alice_msgs[1].content, "msg2");

        let bob_msgs = bus.read_inbox("bob");
        assert_eq!(bob_msgs.len(), 1);
    }

    #[test]
    fn test_message_bus_all_msg_types() {
        let bus = MessageBus::new();
        for msg_type in VALID_MSG_TYPES {
            let result = bus.send("lead", "alice", "test", msg_type);
            assert!(result.is_ok(), "Failed for type: {msg_type}");
        }
        let msgs = bus.read_inbox("alice");
        assert_eq!(msgs.len(), VALID_MSG_TYPES.len());
    }

    #[test]
    fn test_message_bus_read_empty_inbox() {
        let bus = MessageBus::new();
        let msgs = bus.read_inbox("nonexistent");
        assert!(msgs.is_empty());
    }

    // -- TeammateManager tests --

    #[test]
    fn test_teammate_manager_new() {
        let tmp = TempDir::new().unwrap();
        let mgr = TeammateManager::new(tmp.path()).unwrap();
        assert_eq!(mgr.team_name(), "default");
        assert!(mgr.member_names().is_empty());
    }

    #[test]
    fn test_teammate_manager_spawn() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TeammateManager::new(tmp.path()).unwrap();

        let result = mgr.spawn("alice", "coder").unwrap();
        assert!(result.contains("Spawned"));
        assert!(result.contains("alice"));

        let names = mgr.member_names();
        assert_eq!(names, vec!["alice"]);

        let member = mgr.find_member("alice").unwrap();
        assert_eq!(member.role, "coder");
        assert_eq!(member.status, "working");
    }

    #[test]
    fn test_teammate_manager_spawn_existing() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TeammateManager::new(tmp.path()).unwrap();

        mgr.spawn("alice", "coder").unwrap();
        mgr.set_status("alice", "idle");
        let result = mgr.spawn("alice", "tester").unwrap();
        assert!(result.contains("Spawned"));

        let member = mgr.find_member("alice").unwrap();
        assert_eq!(member.role, "tester");
        assert_eq!(member.status, "working");
    }

    #[test]
    fn test_teammate_manager_spawn_busy_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TeammateManager::new(tmp.path()).unwrap();

        mgr.spawn("alice", "coder").unwrap();
        // alice is "working", so spawn should fail
        let result = mgr.spawn("alice", "tester");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("currently working"));
    }

    #[test]
    fn test_teammate_manager_list_all() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TeammateManager::new(tmp.path()).unwrap();

        assert_eq!(mgr.list_all(), "No teammates.");

        mgr.spawn("alice", "coder").unwrap();
        mgr.spawn("bob", "tester").unwrap();
        let list = mgr.list_all();
        assert!(list.contains("Team: default"));
        assert!(list.contains("alice (coder): working"));
        assert!(list.contains("bob (tester): working"));
    }

    #[test]
    fn test_teammate_manager_set_status() {
        let tmp = TempDir::new().unwrap();
        let mut mgr = TeammateManager::new(tmp.path()).unwrap();

        mgr.spawn("alice", "coder").unwrap();
        mgr.set_status("alice", "idle");
        let member = mgr.find_member("alice").unwrap();
        assert_eq!(member.status, "idle");
    }

    #[test]
    fn test_teammate_manager_persistence() {
        let tmp = TempDir::new().unwrap();
        {
            let mut mgr = TeammateManager::new(tmp.path()).unwrap();
            mgr.spawn("alice", "coder").unwrap();
        }
        // Reload from disk
        let mgr = TeammateManager::new(tmp.path()).unwrap();
        assert_eq!(mgr.member_names(), vec!["alice"]);
        let member = mgr.find_member("alice").unwrap();
        assert_eq!(member.role, "coder");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = TeamConfig {
            team_name: "my-team".to_string(),
            members: vec![
                TeamMember {
                    name: "alice".to_string(),
                    role: "coder".to_string(),
                    status: "working".to_string(),
                },
                TeamMember {
                    name: "bob".to_string(),
                    role: "tester".to_string(),
                    status: "idle".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: TeamConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.team_name, "my-team");
        assert_eq!(parsed.members.len(), 2);
        assert_eq!(parsed.members[0].name, "alice");
    }
}
