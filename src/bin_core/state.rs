use crate::agent_teams::{MessageBus, TeammateManager};
use crate::background_tasks::BackgroundManager;
use crate::context_compact::ContextCompactor;
use crate::llm_client::AnthropicClient;
use crate::skill_loading::SkillLoader;
use crate::subagent::Subagent;
use crate::task_system::TaskManager;
use crate::team_protocols::ProtocolTracker;
use crate::todo_manager::TodoManager;
use crate::worktree::WorktreeManager;

use anyhow::Context as AnyhowContext;
use serde_json::Value as Json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Full agent state containing all components.
pub struct State {
    pub client: AnthropicClient,
    pub model: String,
    pub workdir: PathBuf,
    pub todo: Mutex<TodoManager>,
    pub task_mgr: Arc<Mutex<TaskManager>>,
    pub bg: BackgroundManager,
    pub bus: Arc<MessageBus>,
    pub team: Mutex<TeammateManager>,
    pub skills: SkillLoader,
    pub protocols: ProtocolTracker,
    pub compactor: ContextCompactor,
    pub subagent: Subagent,
    pub worktree: WorktreeManager,
}

impl State {
    pub fn new(
        client: AnthropicClient,
        model: String,
        workdir: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let skills_dir = workdir.join("skills").to_string_lossy().to_string();
        let skills = SkillLoader::new(&skills_dir);
        let task_mgr =
            TaskManager::new(&workdir.join(".tasks")).context("failed to open .tasks")?;
        let team = TeammateManager::new(&workdir.join(".team")).context("failed to open .team")?;
        let compactor = ContextCompactor::new(
            AnthropicClient::new(&client.api_key, &client.base_url),
            workdir.to_string_lossy().to_string(),
            model.clone(),
        );
        let subagent = Subagent::new(
            AnthropicClient::new(&client.api_key, &client.base_url),
            workdir.to_string_lossy().to_string(),
            model.clone(),
        );
        let worktree = WorktreeManager::new(&workdir).context("failed to open worktree")?;

        Ok(Self {
            client,
            model,
            workdir,
            todo: Mutex::new(TodoManager::new()),
            task_mgr: Arc::new(Mutex::new(task_mgr)),
            bg: BackgroundManager::new(),
            bus: Arc::new(MessageBus::new()),
            team: Mutex::new(team),
            skills,
            protocols: ProtocolTracker::new(),
            compactor,
            subagent,
            worktree,
        })
    }

    /// Get the full set of tools as a Json array.
    pub fn tools(&self) -> Json {
        Json::Array(crate::tools::full_agent_tools())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> AnthropicClient {
        AnthropicClient::new("test_key", "https://api.anthropic.com")
    }

    #[test]
    fn test_state_creation() {
        let client = test_client();
        let workdir = PathBuf::from("/tmp/test_state");
        let state = State::new(client, "test-model".to_string(), workdir.clone()).unwrap();

        assert_eq!(state.model, "test-model");
        assert_eq!(state.workdir, workdir);
    }

    #[test]
    fn test_state_tools() {
        let client = test_client();
        let workdir = PathBuf::from("/tmp/test_state");
        let state = State::new(client, "test-model".to_string(), workdir).unwrap();

        let tools = state.tools();
        assert!(tools.is_array());

        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 26); // Full agent has 26 tools (23 + 3 worktree)

        // Check for key tools
        let names: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"TodoWrite"));
        assert!(names.contains(&"task"));
        assert!(names.contains(&"task_create"));
        assert!(names.contains(&"spawn_teammate"));
    }
}
