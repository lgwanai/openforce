//! Skill executor — Level 3 execution: scripts, references, adapter tools.

use crate::SkillRegistry;

/// Executor for skill Level 3 operations: loading references, running scripts,
/// invoking adapter.py tools.
pub struct SkillExecutor<'a> {
    registry: &'a SkillRegistry,
}

impl<'a> SkillExecutor<'a> {
    pub fn new(registry: &'a SkillRegistry) -> Self {
        Self { registry }
    }

    /// Load a supporting reference file from a skill's directory.
    /// Only allows files under `references/`, `templates/`, or `assets/`.
    pub fn load_reference(&self, skill_name: &str, relative_path: &str) -> Option<String> {
        let skill = self.registry.get(skill_name)?;
        if !skill.enabled {
            return None;
        }

        // Validate the path is under an allowed subdirectory
        let allowed_prefixes = ["references/", "templates/", "assets/"];
        let is_allowed = allowed_prefixes
            .iter()
            .any(|prefix| relative_path.starts_with(prefix));

        if !is_allowed {
            tracing::warn!("skill reference path not under allowed dirs: {relative_path}");
            return None;
        }

        // Resolve and validate the path (prevent traversal attacks)
        let full_path = skill.dir.join(relative_path);
        let canonical_skill = skill.dir.canonicalize().ok()?;

        // For files that don't exist yet, we can't canonicalize them,
        // but we can check the parent directory
        if let Ok(canonical_target) = full_path.canonicalize() {
            if !canonical_target.starts_with(&canonical_skill) {
                tracing::warn!("skill reference path escapes skill dir: {relative_path}");
                return None;
            }
        } else {
            // File doesn't exist
            return None;
        }

        std::fs::read_to_string(&full_path).ok()
    }

    /// Execute a script from a skill's `scripts/` directory.
    /// Returns the stdout output of the script.
    pub async fn execute_script(
        &self,
        skill_name: &str,
        script_path: &str,
        args: &[&str],
    ) -> Result<String, String> {
        let skill = self
            .registry
            .get(skill_name)
            .ok_or_else(|| format!("skill not found: {skill_name}"))?;

        if !skill.enabled {
            return Err(format!("skill disabled: {skill_name}"));
        }

        // Must be under scripts/
        if !script_path.starts_with("scripts/") {
            return Err(format!("script path must be under scripts/: {script_path}"));
        }

        let full_path = skill.dir.join(script_path);
        if !full_path.exists() {
            return Err(format!("script not found: {:?}", full_path));
        }

        // Validate path stays within skill dir
        if let (Ok(canonical_target), Ok(canonical_skill)) =
            (full_path.canonicalize(), skill.dir.canonicalize())
        {
            if !canonical_target.starts_with(&canonical_skill) {
                return Err(format!("script path escapes skill dir: {script_path}"));
            }
        }

        // Determine interpreter based on extension
        let cmd: String;
        let mut cmd_args: Vec<String>;
        match full_path.extension().and_then(|e| e.to_str()) {
            Some("py") => {
                cmd = "python3".to_string();
                cmd_args = vec![full_path.display().to_string()];
            }
            Some("sh") => {
                cmd = "bash".to_string();
                cmd_args = vec![full_path.display().to_string()];
            }
            Some("js") => {
                cmd = "node".to_string();
                cmd_args = vec![full_path.display().to_string()];
            }
            _ => {
                // Try executing directly
                cmd = full_path.display().to_string();
                cmd_args = vec![];
            }
        }

        cmd_args.extend(args.iter().map(|a| a.to_string()));

        let output = tokio::process::Command::new(cmd)
            .args(&cmd_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("script execution failed: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(format!("script exited with {}: {stderr}", output.status));
        }

        Ok(stdout)
    }

    /// Invoke a tool via a skill's `adapter.py` (Level 3 tool invocation).
    /// Compatible with the existing adapter.py protocol:
    ///   stdin: {"tool": "<name>", "args": {...}}
    ///   stdout: JSON result
    pub async fn invoke_tool(
        &self,
        skill_name: &str,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let skill = self
            .registry
            .get(skill_name)
            .ok_or_else(|| format!("skill not found: {skill_name}"))?;

        if !skill.enabled {
            return Err(format!("skill disabled: {skill_name}"));
        }

        // Check allowed-tools policy
        if !skill.is_tool_allowed(tool_name) {
            return Err(format!(
                "tool '{tool_name}' not allowed by skill '{skill_name}'"
            ));
        }

        let adapter = skill.dir.join("adapter.py");
        if !adapter.exists() {
            return Err(format!("no adapter.py for skill: {skill_name}"));
        }

        let input = serde_json::to_string(&serde_json::json!({"tool": tool_name, "args": args}))
            .unwrap_or_default();

        let mut child = tokio::process::Command::new("python3")
            .arg(&adapter)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn adapter: {e}"))?;

        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes()).await;
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("adapter execution: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(format!("adapter failed: {stderr}"));
        }

        Ok(stdout)
    }

    /// Resolve a search query by trying web_search tool from available skills.
    pub async fn resolve_search(&self, query: &str) -> String {
        for name in self.registry.enabled_skill_names() {
            let adapter_path = match self.registry.get(&name) {
                Some(s) => s.dir.join("adapter.py"),
                None => continue,
            };
            if adapter_path.exists() {
                match self
                    .invoke_tool(
                        &name,
                        "web_search",
                        &serde_json::json!({"query": query, "max_results": 3}),
                    )
                    .await
                {
                    Ok(result) if !result.is_empty() && !result.starts_with('[') => {
                        return result;
                    }
                    _ => continue,
                }
            }
        }
        format!("[需网络检索: {query}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SkillRegistry;
    use std::fs;

    #[test]
    fn test_load_reference_not_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("test-skill");
        fs::create_dir_all(skill_dir.join("references")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: test\n---\nBody",
        )
        .unwrap();
        fs::write(skill_dir.join("references/guide.md"), "Guide content").unwrap();

        let registry = SkillRegistry::discover(dir.path().to_str().unwrap());
        let executor = SkillExecutor::new(&registry);

        // Allowed
        let result = executor.load_reference("test-skill", "references/guide.md");
        assert_eq!(result, Some("Guide content".to_string()));

        // Not allowed (not under allowed dirs)
        let result = executor.load_reference("test-skill", "SKILL.md");
        assert_eq!(result, None);

        // Not allowed (traversal)
        let result = executor.load_reference("test-skill", "../../etc/passwd");
        assert_eq!(result, None);
    }
}
