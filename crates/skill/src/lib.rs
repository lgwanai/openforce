//! OpenForce Skill System — Standard SKILL.md progressive disclosure mechanism.
//!
//! Compatible with Claude Code / Open Claw SKILL.md specification.
//! Three-level progressive loading:
//!   Level 1: Frontmatter metadata (name + description) — loaded at startup
//!   Level 2: Full SKILL.md body — loaded when skill is matched
//!   Level 3: Supporting files (scripts/, references/, assets/) — loaded on demand

mod config;
mod discovery;
mod executor;
mod parser;

pub use config::SkillConfig;
pub use discovery::SkillRegistry;
pub use executor::SkillExecutor;

use std::path::PathBuf;

/// Standard SKILL.md frontmatter fields.
/// Compatible with Claude Code / Open Claw specification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillFrontmatter {
    /// Required: Unique skill name (lowercase, hyphens, max 64 chars).
    pub name: String,
    /// Required: Description used for skill matching (max 1024 chars).
    pub description: String,
    /// Optional: Tools this skill is allowed to use.
    #[serde(rename = "allowed-tools", default)]
    pub allowed_tools: Option<Vec<String>>,
    /// Optional: Skill version.
    #[serde(default)]
    pub version: Option<String>,
    /// Optional: Skill author.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional: Arbitrary metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Optional: License identifier.
    #[serde(default)]
    pub license: Option<String>,
}

/// A discovered skill with its frontmatter, body, and directory location.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Parsed frontmatter metadata.
    pub frontmatter: SkillFrontmatter,
    /// Full body of SKILL.md (after frontmatter).
    pub body: String,
    /// Absolute path to the skill directory.
    pub dir: PathBuf,
    /// Whether this skill is enabled (controlled by config).
    pub enabled: bool,
}

impl Skill {
    /// Check if a specific tool is allowed by this skill's allowed-tools policy.
    /// Returns true if no allowed-tools is specified (legacy allow-all behavior).
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        match &self.frontmatter.allowed_tools {
            Some(allowed) => allowed.iter().any(|t| t == tool_name),
            None => true,
        }
    }

    /// List supporting subdirectories that exist for this skill.
    pub fn available_support_dirs(&self) -> Vec<&'static str> {
        let dirs = ["scripts", "references", "templates", "assets"];
        dirs.iter()
            .filter(|d| self.dir.join(*d).is_dir())
            .copied()
            .collect()
    }

    /// Resolve a relative path within this skill's directory.
    /// Returns None if the path would escape the skill directory.
    pub fn resolve_safe_path(&self, relative: &str) -> Option<PathBuf> {
        let target = self.dir.join(relative);
        let canonical = target.parent()?.canonicalize().ok()?;
        let skill_canonical = self.dir.canonicalize().ok()?;
        if canonical.starts_with(&skill_canonical) {
            Some(target)
        } else {
            None
        }
    }
}
