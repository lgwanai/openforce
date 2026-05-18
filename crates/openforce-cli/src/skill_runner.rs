use std::collections::HashMap;
use std::path::Path;

/// Progressive disclosure: only frontmatter is loaded initially.
/// Full body is loaded on-demand when the LLM decides the skill is needed.
#[derive(Debug, Clone)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub dir: String,
}

/// SkillRunner: progressive disclosure skill system.
/// 1. Discover: scan skills/ for SKILL.md, extract frontmatter only
/// 2. Present: inject available skill descriptions into Planner prompt
/// 3. Load: when the Planner/Worker decides a skill is needed, load its full body
pub struct SkillRunner {
    skills: HashMap<String, Skill>,
}

impl SkillRunner {
    /// Scan `skills/` directory. Only reads SKILL.md frontmatter (progressive disclosure).
    pub fn discover(skills_dir: &str) -> Self {
        let mut skills = HashMap::new();
        let dir = Path::new(skills_dir);
        if !dir.exists() { return Self { skills }; }

        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() { continue; }

            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                if let Some(fm) = Self::parse_frontmatter(&content) {
                    let body = Self::extract_body(&content);
                    let dir = path.display().to_string();
                    tracing::info!("skill discovered: {} — {}", fm.name, fm.description.chars().take(80).collect::<String>());
                    skills.insert(fm.name.clone(), Skill { frontmatter: fm, body, dir });
                }
            }
        }
        Self { skills }
    }

    /// Parse YAML frontmatter manually (no external crate needed).
    fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 { return None; }
        let mut name = String::new();
        let mut description = String::new();
        for line in parts[1].lines() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("name:") { name = v.trim().trim_matches('"').into(); }
            if let Some(v) = t.strip_prefix("description:") { description = v.trim().trim_matches('"').into(); }
        }
        if name.is_empty() { return None; }
        Some(SkillFrontmatter { name, description })
    }

    /// Extract body after frontmatter (second --- to end).
    fn extract_body(content: &str) -> String {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 { return content.to_string(); }
        parts[2].trim().to_string()
    }

    /// All available skill names and descriptions for the Planner prompt.
    /// Only frontmatter is disclosed — LLM decides relevance.
    pub fn skill_summary(&self) -> String {
        if self.skills.is_empty() { return String::new(); }
        let mut s = String::from("AVAILABLE SKILLS:\n");
        for sk in self.skills.values() {
            let desc: String = sk.frontmatter.description.chars().take(120).collect();
            s.push_str(&format!("  {} — {}\n", sk.frontmatter.name, desc));
        }
        s.push_str("  → If relevant to your task, reference the skill by name. Its full instructions will be loaded.\n");
        s
    }

    /// Check if a skill is available by name.
    pub fn has(&self, name: &str) -> bool { self.skills.contains_key(name) }

    /// Load a skill's full body (instructions for the LLM to execute).
    /// Returns the body text to inject into the worker/planner context.
    pub fn load_body(&self, name: &str) -> Option<String> {
        self.skills.get(name).map(|s| s.body.clone())
    }

    /// For skills that expose tools (like deerflow's web_search), invoke via adapter.
    pub async fn invoke_tool(&self, skill_name: &str, tool_name: &str, args: &serde_json::Value) -> String {
        let skill = match self.skills.get(skill_name) { Some(s) => s, None => return format!("[skill not found: {skill_name}]") };
        let adapter = format!("{}/adapter.py", skill.dir);
        if !Path::new(&adapter).exists() { return format!("[no adapter for {skill_name}]"); }

        let input = serde_json::to_string(&serde_json::json!({"tool":tool_name,"args":args})).unwrap_or_default();
        match tokio::process::Command::new("python3").arg(&adapter)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        {
            Ok(mut child) => {
                use tokio::io::AsyncWriteExt;
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(input.as_bytes()).await; drop(s); }
                match child.wait_with_output().await {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                    Err(e) => format!("[error: {e}]"),
                }
            }
            Err(e) => format!("[spawn error: {e}]"),
        }
    }

    /// Resolve [需网络检索: query] by trying web_search tool from available skills.
    pub async fn resolve_search(&self, query: &str) -> String {
        // Check if any skill provides web_search
        for (name, skill) in &self.skills {
            let adapter = format!("{}/adapter.py", skill.dir);
            if Path::new(&adapter).exists() {
                let result = self.invoke_tool(name, "web_search", &serde_json::json!({"query": query, "max_results": 3})).await;
                if !result.starts_with('[') && !result.is_empty() {
                    return result;
                }
            }
        }
        format!("[需网络检索: {query}]")
    }
}
