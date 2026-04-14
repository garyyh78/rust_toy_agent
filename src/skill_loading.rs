use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub tags: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    pub body: String,
    pub path: String,
}

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
                            skills.insert(
                                name,
                                Skill {
                                    meta,
                                    body,
                                    path: path.to_string_lossy().to_string(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

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

    pub fn get_content(&self, name: &str) -> String {
        match self.skills.get(name) {
            Some(skill) => format!("<skill name=\"{}\">\n{}\n</skill>", name, skill.body),
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

    pub fn list_skills(&self) -> Vec<&str> {
        let mut skills: Vec<&str> = self.skills.keys().map(|k| k.as_str()).collect();
        skills.sort();
        skills
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_skill_loader_parse_frontmatter() {
        let text =
            "---\nname: test\n\ndescription: Test skill\ntags: testing\n---\nSkill body here";
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
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_dir = tmp.path().join("test_skills");
        let skill_dir = skills_dir.join("test_skill");

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
    }
}
