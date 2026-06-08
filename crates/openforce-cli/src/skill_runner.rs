//! Backward-compatible re-export of the openforce-skill crate.
//!
//! This module re-exports the new skill system types so that existing
//! code using `crate::skill_runner::SkillRunner` continues to work
//! during the migration period.

// Re-export the new types
pub use openforce_skill::{SkillConfig, SkillExecutor, SkillRegistry};

/// Backward-compatible wrapper around SkillRegistry.
/// Preserves the old `SkillRunner` API while delegating to the new crate.
pub struct SkillRunner {
    registry: SkillRegistry,
}

#[allow(dead_code)]
impl SkillRunner {
    /// Discover skills in the given directory.
    pub fn discover(skills_dir: &str) -> Self {
        let config = SkillConfig::load_from_skills_dir(skills_dir);
        let registry = SkillRegistry::discover_with_config(skills_dir, Some(&config));
        Self { registry }
    }

    /// Get the Level 1 metadata prompt (replaces old skill_summary).
    pub fn has_skills(&self) -> bool {
        !self.registry.is_empty()
    }

    pub fn skill_summary(&self) -> String {
        self.registry.metadata_prompt()
    }

    /// Check if a skill is available by name.
    pub fn has(&self, name: &str) -> bool {
        self.registry.has(name)
    }

    /// Load a skill's full body (Level 2).
    pub fn load_body(&self, name: &str) -> Option<String> {
        self.registry.load_skill_body(name)
    }

    /// Invoke a tool via a skill's adapter.py (Level 3).
    pub async fn invoke_tool(
        &self,
        skill_name: &str,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> String {
        let executor = SkillExecutor::new(&self.registry);
        match executor.invoke_tool(skill_name, tool_name, args).await {
            Ok(result) => result,
            Err(e) => format!("[{e}]"),
        }
    }

    /// Resolve a search query by trying web_search from available skills.
    pub async fn resolve_search(&self, query: &str) -> String {
        let executor = SkillExecutor::new(&self.registry);
        executor.resolve_search(query).await
    }

    /// Get a reference to the underlying registry for advanced usage.
    pub fn registry(&self) -> &SkillRegistry {
        &self.registry
    }
}
