use crate::client::AnthropicClient;
use crate::help_utils::{run_bash, run_edit, run_read, run_write};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::Path;

/// Skill metadata
#[derive(Debug, Clone)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub tags: String,
}

/// Skill content
#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    pub body: String,
    pub path: String,
}

/// SkillLoader - Two-layer skill injection that avoids bloating the system prompt.
/// Layer 1 (cheap): skill names in system prompt (~100 tokens/skill)
/// Layer 2 (on demand): full skill body in tool_result
pub struct SkillLoader {
    skills_dir: String,
    skills: HashMap<String, Skill>,
}

impl SkillLoader {
    pub fn new(skills_dir: &str) -> Self {
        let mut loader = Self {
            skills_dir: skills_dir.to_string(),
            skills: HashMap::new(),
        };
        loader.load_all();
        loader
    }
    
    /// Load all skills from the skills directory
    fn load_all(&mut self) {
        let skills_dir = self.skills_dir.clone();
        let skills_path = Path::new(&skills_dir);
        if !skills_path.exists() {
            return;
        }
        
        let mut skills = HashMap::new();
        Self::walk_directory_static(skills_path, &mut skills);
        self.skills = skills;
    }
    
    /// Recursively walk directory to find SKILL.md files (static version)
    fn walk_directory_static(dir: &Path, skills: &mut HashMap<String, Skill>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::walk_directory_static(&path, skills);
                } else if let Some(file_name) = path.file_name() {
                    if file_name == "SKILL.md" {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let (meta, body) = Self::parse_frontmatter_static(&content);
                            let name = meta.name.clone();
                            skills.insert(name, Skill {
                                meta,
                                body,
                                path: path.to_string_lossy().to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    
    /// Parse YAML frontmatter between --- delimiters
    fn parse_frontmatter_static(text: &str) -> (SkillMeta, String) {
        let parts: Vec<&str> = text.splitn(3, "---\n").collect();
        if parts.len() < 3 {
            return (
                SkillMeta {
                    name: "unknown".to_string(),
                    description: "No description".to_string(),
                    tags: String::new(),
                },
                text.to_string(),
            );
        }
        
        let frontmatter = parts[1];
        let body = parts[2].trim().to_string();
        
        let mut name = "unknown".to_string();
        let mut description = "No description".to_string();
        let mut tags = String::new();
        
        for line in frontmatter.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                match key {
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "tags" => tags = value.to_string(),
                    _ => {}
                }
            }
        }
        
        (
            SkillMeta {
                name,
                description,
                tags,
            },
            body,
        )
    }
    
    /// Parse YAML frontmatter between --- delimiters (instance method)
    fn parse_frontmatter(&self, text: &str) -> (SkillMeta, String) {
        Self::parse_frontmatter_static(text)
    }
    
    /// Layer 1: short descriptions for the system prompt
    pub fn get_descriptions(&self) -> String {
        if self.skills.is_empty() {
            return "(no skills available)".to_string();
        }
        
        let mut lines = Vec::new();
        for (name, skill) in &self.skills {
            let mut line = format!("  - {}: {}", name, skill.meta.description);
            if !skill.meta.tags.is_empty() {
                line.push_str(&format!(" [{}]", skill.meta.tags));
            }
            lines.push(line);
        }
        
        lines.join("\n")
    }
    
    /// Layer 2: full skill body returned in tool_result
    pub fn get_content(&self, name: &str) -> String {
        match self.skills.get(name) {
            Some(skill) => format!(
                "<skill name=\"{}\">\n{}\n</skill>",
                name, skill.body
            ),
            None => {
                let available: Vec<&str> = self.skills.keys().map(|k| k.as_str()).collect();
                format!(
                    "Error: Unknown skill '{}'. Available: {}",
                    name,
                    available.join(", ")
                )
            }
        }
    }
    
    /// Get list of available skill names
    pub fn list_skills(&self) -> Vec<&str> {
        self.skills.keys().map(|k| k.as_str()).collect()
    }
    
    /// Check if a skill exists
    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }
}

/// Skill loading agent with tools
pub struct SkillAgent {
    client: AnthropicClient,
    workdir: String,
    model: String,
    skill_loader: SkillLoader,
    tools: Json,
}

impl SkillAgent {
    pub fn new(client: AnthropicClient, workdir: String, model: String, skills_dir: String) -> Self {
        let skill_loader = SkillLoader::new(&skills_dir);
        
        let tools = serde_json::json!([
            {
                "name": "bash",
                "description": "Run a shell command.",
                "input_schema": {
                    "type": "object",
                    "properties": {"command": {"type": "string"}},
                    "required": ["command"]
                }
            },
            {
                "name": "read_file",
                "description": "Read file contents.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "limit": {"type": "integer"}
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "write_file",
                "description": "Write content to file.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"]
                }
            },
            {
                "name": "edit_file",
                "description": "Replace exact text in file.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "old_text": {"type": "string"},
                        "new_text": {"type": "string"}
                    },
                    "required": ["path", "old_text", "new_text"]
                }
            },
            {
                "name": "load_skill",
                "description": "Load specialized knowledge by name.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Skill name to load"}
                    },
                    "required": ["name"]
                }
            }
        ]);
        
        Self {
            client,
            workdir,
            model,
            skill_loader,
            tools,
        }
    }
    
    /// Dispatch a tool call
    fn dispatch_tool(&self, tool_name: &str, input: &Json) -> String {
        let workdir = Path::new(&self.workdir);
        match tool_name {
            "bash" => run_bash(input["command"].as_str().unwrap_or(""), workdir),
            "read_file" => run_read(
                input["path"].as_str().unwrap_or(""),
                input["limit"].as_u64().map(|n| n as usize),
                workdir,
            ),
            "write_file" => run_write(
                input["path"].as_str().unwrap_or(""),
                input["content"].as_str().unwrap_or(""),
                workdir,
            ),
            "edit_file" => run_edit(
                input["path"].as_str().unwrap_or(""),
                input["old_text"].as_str().unwrap_or(""),
                input["new_text"].as_str().unwrap_or(""),
                workdir,
            ),
            "load_skill" => {
                let name = input["name"].as_str().unwrap_or("");
                self.skill_loader.get_content(name)
            }
            _ => format!("Unknown tool: {}", tool_name),
        }
    }
    
    /// Get system prompt with skill descriptions
    pub fn get_system_prompt(&self) -> String {
        format!(
            "You are a coding agent at {}.\n\
             Use load_skill to access specialized knowledge before tackling unfamiliar topics.\n\n\
             Skills available:\n{}",
            self.workdir,
            self.skill_loader.get_descriptions()
        )
    }
    
    /// Main agent loop (async)
    pub async fn agent_loop(&self, messages: &mut Vec<Json>) {
        let system = self.get_system_prompt();
        
        loop {
            let response = self.client.create_message(
                &self.model,
                Some(&system),
                messages,
                Some(&self.tools),
                8000,
            ).await;
            
            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    println!("Error: {}", e);
                    return;
                }
            };
            
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": response["content"]
            }));
            
            if response["stop_reason"] != "tool_use" {
                return;
            }
            
            let mut results = Vec::new();
            if let Some(content) = response["content"].as_array() {
                for block in content {
                    if block["type"] == "tool_use" {
                        let tool_name = block["name"].as_str().unwrap_or("");
                        let input = &block["input"];
                        
                        let output = self.dispatch_tool(tool_name, input);
                        
                        println!("> {}: {}", tool_name, &output[..std::cmp::min(200, output.len())]);
                        
                        results.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": block["id"],
                            "content": output
                        }));
                    }
                }
            }
            
            messages.push(serde_json::json!({
                "role": "user",
                "content": results
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;
    
    #[test]
    fn test_skill_loader_parse_frontmatter() {
        let text = "---\nname: test\n\ndescription: Test skill\ntags: testing\n---\nSkill body here";
        let (meta, body) = SkillLoader::parse_frontmatter_static(text);
        
        assert_eq!(meta.name, "test");
        assert_eq!(meta.description, "Test skill");
        assert_eq!(meta.tags, "testing");
        assert_eq!(body, "Skill body here");
    }
    
    #[test]
    fn test_skill_loader_no_frontmatter() {
        let text = "No frontmatter here";
        let (meta, body) = SkillLoader::parse_frontmatter_static(text);
        
        assert_eq!(meta.name, "unknown");
        assert_eq!(body, "No frontmatter here");
    }
    
    #[test]
    fn test_skill_loader_empty() {
        let loader = SkillLoader::new("/nonexistent");
        
        assert!(loader.skills.is_empty());
        assert_eq!(loader.get_descriptions(), "(no skills available)");
    }
    
    #[test]
    fn test_skill_loader_with_skills() {
        let temp_dir = env::temp_dir();
        let skills_dir = temp_dir.join("test_skills");
        let skill_dir = skills_dir.join("test_skill");
        
        // Create skill directory and file
        let _ = fs::create_dir_all(&skill_dir);
        let skill_content = "---\nname: test\n\ndescription: Test skill\n---\nSkill body";
        let _ = fs::write(skill_dir.join("SKILL.md"), skill_content);
        
        let loader = SkillLoader::new(skills_dir.to_str().unwrap());
        
        assert_eq!(loader.list_skills(), vec!["test"]);
        assert!(loader.has_skill("test"));
        assert!(!loader.has_skill("nonexistent"));
        
        let descriptions = loader.get_descriptions();
        assert!(descriptions.contains("test: Test skill"));
        
        let content = loader.get_content("test");
        assert!(content.contains("<skill name=\"test\">"));
        assert!(content.contains("Skill body"));
        
        let unknown = loader.get_content("unknown");
        assert!(unknown.contains("Error: Unknown skill"));
        
        // Cleanup
        let _ = fs::remove_dir_all(skills_dir);
    }
    
    #[test]
    fn test_skill_agent_creation() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let agent = SkillAgent::new(client, "/tmp".to_string(), "test-model".to_string(), "/nonexistent".to_string());
        
        assert_eq!(agent.workdir, "/tmp");
        assert_eq!(agent.model, "test-model");
        
        // Verify tools
        let tools = agent.tools.as_array().unwrap();
        assert_eq!(tools.len(), 5);
        assert_eq!(tools[4]["name"], "load_skill");
    }
    
    #[test]
    fn test_skill_agent_dispatch() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let agent = SkillAgent::new(client, "/tmp".to_string(), "test-model".to_string(), "/nonexistent".to_string());
        
        // Test bash tool
        let input = serde_json::json!({"command": "echo hello"});
        let result = agent.dispatch_tool("bash", &input);
        assert!(result.contains("hello"));
        
        // Test load_skill (nonexistent)
        let input = serde_json::json!({"name": "nonexistent"});
        let result = agent.dispatch_tool("load_skill", &input);
        assert!(result.contains("Error: Unknown skill"));
        
        // Test unknown tool
        let result = agent.dispatch_tool("unknown", &serde_json::json!({}));
        assert!(result.contains("Unknown tool"));
    }
    
    #[test]
    fn test_skill_agent_system_prompt() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let agent = SkillAgent::new(client, "/tmp".to_string(), "test-model".to_string(), "/nonexistent".to_string());
        
        let system = agent.get_system_prompt();
        assert!(system.contains("coding agent at /tmp"));
        assert!(system.contains("load_skill"));
        assert!(system.contains("no skills available"));
    }
}