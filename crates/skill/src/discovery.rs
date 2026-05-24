//! Skill discovery — scan directories for SKILL.md files and build a registry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::SkillConfig;
use crate::parser;
use crate::Skill;

/// Registry of discovered skills, supporting three-level progressive disclosure.
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    /// Scan a directory for skills (Level 1 discovery).
    ///
    /// Walks the top-level subdirectories of `skills_dir`, looking for `SKILL.md`
    /// files. Only frontmatter metadata and body are loaded; supporting resources
    /// (scripts/, references/, etc.) are discovered but not read until Level 3.
    pub fn discover(skills_dir: &str) -> Self {
        Self::discover_with_config(skills_dir, None)
    }

    /// Scan with optional configuration for enable/disable overrides.
    pub fn discover_with_config(skills_dir: &str, config: Option<&SkillConfig>) -> Self {
        let mut skills = HashMap::new();
        let dir = Path::new(skills_dir);
        if !dir.exists() {
            tracing::info!("skills directory not found: {skills_dir}");
            return Self {
                skills,
                skills_dir: dir.to_path_buf(),
            };
        }

        for entry in walk_skills_dir(dir) {
            let skill_md = entry.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            match std::fs::read_to_string(&skill_md) {
                Ok(content) => {
                    if let Some(fm) = parser::parse_frontmatter(&content) {
                        let body = parser::extract_body(&content);
                        let enabled = config
                            .map(|c| c.is_enabled(&fm.name))
                            .unwrap_or(true);

                        if enabled {
                            tracing::info!(
                                "skill discovered: {} — {}",
                                fm.name,
                                fm.description.chars().take(80).collect::<String>()
                            );
                        } else {
                            tracing::info!("skill disabled: {}", fm.name);
                        }

                        skills.insert(
                            fm.name.clone(),
                            Skill {
                                frontmatter: fm,
                                body,
                                dir: entry,
                                enabled,
                            },
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to read {:?}: {e}", skill_md);
                }
            }
        }

        Self {
            skills,
            skills_dir: dir.to_path_buf(),
        }
    }

    /// Generate the Level 1 metadata prompt for injection into system prompts.
    /// Only includes name + description of enabled skills (~100 tokens/skill).
    pub fn metadata_prompt(&self) -> String {
        let enabled: Vec<&Skill> = self.skills.values().filter(|s| s.enabled).collect();
        if enabled.is_empty() { return String::new(); }
        let mut s = format!("<available_skills total=\"{}\">\n", enabled.len());
        for sk in &enabled {
            let desc: String = sk.frontmatter.description.chars().take(150).collect();
            s.push_str(&format!(
                "  <skill name=\"{}\">{}</skill>\n",
                sk.frontmatter.name.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;"),
                desc.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;")
            ));
        }
        s.push_str("</available_skills>\n");
        s.push_str("Use skill_load tool to load a skill when relevant.\n");
        s
    }

    /// Check if a skill is available and enabled by name.
    pub fn has(&self, name: &str) -> bool {
        self.skills
            .get(name)
            .map(|s| s.enabled)
            .unwrap_or(false)
    }

    /// Load a skill's full body (Level 2 — instructions for the LLM to execute).
    /// Returns None if the skill doesn't exist or is disabled.
    pub fn load_skill_body(&self, name: &str) -> Option<String> {
        self.skills.get(name).and_then(|s| {
            if s.enabled {
                Some(s.body.clone())
            } else {
                None
            }
        })
    }

    /// Get a reference to a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// List all discovered skill names (including disabled).
    pub fn skill_names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    /// List only enabled skill names.
    pub fn enabled_skill_names(&self) -> Vec<String> {
        self.skills
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Number of discovered skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Get the skills directory path.
    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }
}

/// Walk the top-level subdirectories of a skills directory.
/// Only enters directories that are not hidden (don't start with `.`).
fn walk_skills_dir(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip hidden directories (e.g., .venv, .git)
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.starts_with('.'))
        {
            continue;
        }
        // Check if this directory or any subdirectory contains SKILL.md
        if path.join("SKILL.md").exists() {
            result.push(path);
        } else {
            // Also check one level deeper for nested skill directories
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub_entry in sub_entries.flatten() {
                    let sub_path = sub_entry.path();
                    if sub_path.is_dir() && sub_path.join("SKILL.md").exists() {
                        result.push(sub_path);
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let registry = SkillRegistry::discover(dir.path().to_str().unwrap());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_discover_nonexistent_dir() {
        let registry = SkillRegistry::discover("/nonexistent/skills");
        assert!(registry.is_empty());
    }

    #[test]
    fn test_metadata_prompt_empty() {
        let dir = tempfile::tempdir().unwrap();
        let registry = SkillRegistry::discover(dir.path().to_str().unwrap());
        assert!(registry.metadata_prompt().is_empty());
    }

    #[test]
    fn test_discover_skill_with_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("full-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: full-skill\ndescription: A fully specified skill\nallowed-tools:\n  - web_search\n  - web_fetch\nversion: \"2.0\"\nauthor: test-author\nlicense: Apache-2.0\n---\n# Full Skill\nComplete instructions.",
        ).unwrap();

        let registry = SkillRegistry::discover(dir.path().to_str().unwrap());
        assert_eq!(registry.len(), 1);
        assert!(registry.has("full-skill"));
        let skill = registry.get("full-skill").unwrap();
        assert_eq!(skill.frontmatter.allowed_tools.as_deref(), Some(&["web_search".to_string(), "web_fetch".to_string()][..]));
        assert_eq!(skill.frontmatter.version.as_deref(), Some("2.0"));
        assert_eq!(skill.frontmatter.author.as_deref(), Some("test-author"));
        assert_eq!(skill.frontmatter.license.as_deref(), Some("Apache-2.0"));
        assert!(skill.is_tool_allowed("web_search"));
        assert!(!skill.is_tool_allowed("dangerous_tool"));
        assert!(registry.metadata_prompt().contains("full-skill"));
    }
}
