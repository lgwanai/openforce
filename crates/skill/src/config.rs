//! Skill configuration — enable/disable skills and tool access control.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Per-skill override configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOverride {
    /// Whether this skill is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional override for allowed-tools.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

/// Skills configuration loaded from `.skills-config.json`.
///
/// Example:
/// ```json
/// {
///   "disabled_skills": ["risky-skill"],
///   "skill_overrides": {
///     "deerflow-skill": { "enabled": true, "allowed_tools": ["web_search"] }
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// Globally disabled skill names.
    #[serde(default)]
    pub disabled_skills: Vec<String>,
    /// Per-skill overrides.
    #[serde(default)]
    pub skill_overrides: HashMap<String, SkillOverride>,
}

impl SkillConfig {
    /// Load config from a file. Returns default config if the file doesn't exist.
    pub fn load(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            match serde_json::from_str(&content) {
                Ok(config) => {
                    tracing::info!("loaded skill config from {:?}", path);
                    return config;
                }
                Err(e) => {
                    tracing::warn!("invalid skill config {:?}: {e}", path);
                }
            }
        }
        Self::default()
    }

    /// Load config from the skills directory (`.skills-config.json`).
    pub fn load_from_skills_dir(skills_dir: &str) -> Self {
        let path = Path::new(skills_dir).join(".skills-config.json");
        Self::load(&path)
    }

    /// Check if a skill is enabled.
    /// Override takes precedence over the disabled list.
    pub fn is_enabled(&self, skill_name: &str) -> bool {
        // Check overrides first (they take precedence)
        if let Some(override_cfg) = self.skill_overrides.get(skill_name) {
            return override_cfg.enabled;
        }
        // Then check disabled list
        if self.disabled_skills.iter().any(|d| d == skill_name) {
            return false;
        }
        // Default: enabled
        true
    }

    /// Get the effective allowed-tools for a skill, considering overrides.
    /// Returns None if no override is specified (use the skill's own allowed-tools).
    pub fn get_allowed_tools(&self, skill_name: &str) -> Option<&Vec<String>> {
        self.skill_overrides
            .get(skill_name)
            .and_then(|o| o.allowed_tools.as_ref())
    }
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            disabled_skills: Vec::new(),
            skill_overrides: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_all_enabled() {
        let config = SkillConfig::default();
        assert!(config.is_enabled("any-skill"));
    }

    #[test]
    fn test_disabled_skill() {
        let config = SkillConfig {
            disabled_skills: vec!["risky".to_string()],
            skill_overrides: HashMap::new(),
        };
        assert!(!config.is_enabled("risky"));
        assert!(config.is_enabled("safe"));
    }

    #[test]
    fn test_override_enables_disabled() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "risky".to_string(),
            SkillOverride {
                enabled: true,
                allowed_tools: None,
            },
        );
        let config = SkillConfig {
            disabled_skills: vec!["risky".to_string()],
            skill_overrides: overrides,
        };
        // Override takes precedence
        assert!(config.is_enabled("risky"));
    }

    #[test]
    fn test_override_allowed_tools() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "deploy".to_string(),
            SkillOverride {
                enabled: true,
                allowed_tools: Some(vec!["web_search".to_string()]),
            },
        );
        let config = SkillConfig {
            disabled_skills: vec![],
            skill_overrides: overrides,
        };
        assert_eq!(
            config.get_allowed_tools("deploy"),
            Some(&vec!["web_search".to_string()])
        );
        assert_eq!(config.get_allowed_tools("other"), None);
    }
}
