use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

// ── Agent Profile ──

/// A loaded agent profile with full system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Agent display name (e.g. "Backend Architect") — exact match key
    pub name: String,
    /// Domain category (e.g. "engineering", "design", "marketing")
    pub domain: String,
    /// Short description for planner selection
    pub description: String,
    /// The full agent personality document — serves as the Worker's system prompt
    pub system_prompt: String,
    /// Optional tools hint (comma-separated from frontmatter)
    pub tools_hint: Option<String>,
    /// Visual identifier
    pub emoji: Option<String>,
    /// Vibe line from frontmatter
    pub vibe: Option<String>,
    /// File path for reference
    pub source_file: String,
}

/// Full agent registry containing all agent profiles.
/// Uses progressive disclosure: Level 1 metadata in planner context,
/// Level 2 full profile loaded on demand for Worker creation.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    pub agents: Vec<AgentProfile>,
    /// Index: domain → agents
    by_domain: HashMap<String, Vec<usize>>,
    /// Index: name → index (exact match only — progressive disclosure)
    by_name: HashMap<String, usize>,
}

impl AgentRegistry {
    /// Load all agent files from the agents/ directory.
    pub fn load(agents_dir: &Path) -> Result<Self, String> {
        let mut agents = Vec::new();
        let mut by_domain: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();

        if !agents_dir.exists() {
            return Err(format!("agents dir not found: {:?}", agents_dir));
        }

        let mut entries: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = fs::read_dir(agents_dir) {
            for entry in dir.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "md") {
                    entries.push(path);
                }
            }
        }

        for path in &entries {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (frontmatter, body) = parse_frontmatter(&content);
            let domain = path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.split('_').next().unwrap_or("unknown"))
                .unwrap_or("unknown")
                .to_string();

            let name = frontmatter.get("name")
                .cloned()
                .unwrap_or_else(|| "Unknown Agent".into());

            let description = frontmatter.get("description")
                .cloned()
                .unwrap_or_else(|| "No description".into());

            let tools_hint = frontmatter.get("tools").cloned();
            let emoji = frontmatter.get("emoji").cloned();
            let vibe = frontmatter.get("vibe").cloned();

            let idx = agents.len();
            agents.push(AgentProfile {
                name: name.clone(),
                domain: domain.clone(),
                description,
                system_prompt: body.to_string(),
                tools_hint,
                emoji,
                vibe,
                source_file: path.display().to_string(),
            });

            by_name.insert(name.clone(), idx);
            by_domain.entry(domain).or_default().push(idx);
        }

        eprintln!("[AgentRegistry] {} agents in {} domains", agents.len(), by_domain.len());
        Ok(Self { agents, by_domain, by_name })
    }

    // ── Progressive Disclosure: Level 1 — Metadata for Planner ──

    /// Generate an XML-format agent catalog for the Planner context window.
    /// Level 1 (metadata only): name + description + domain + emoji.
    /// The Planner selects exact agent names from this listing.
    /// Level 2 (full profile) is loaded via `get()` when creating Workers.
    pub fn metadata_for_planner(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "<available_agents total=\"{}\" domains=\"{}\">\n",
            self.agents.len(),
            self.by_domain.len()
        ));

        // Group by domain
        let mut domains: Vec<&String> = self.by_domain.keys().collect();
        domains.sort();

        for domain in domains {
            out.push_str(&format!("  <domain name=\"{domain}\">\n"));

            let agent_indices = self.by_domain.get(domain)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            // Sort agents alphabetically within domain
            let mut sorted: Vec<usize> = agent_indices.to_vec();
            sorted.sort_by(|a, b| self.agents[*a].name.cmp(&self.agents[*b].name));

            for idx in sorted {
                let a = &self.agents[idx];
                let emoji = a.emoji.as_deref().unwrap_or("");
                let desc = truncate_str(&a.description, 120);
                out.push_str(&format!(
                    "    <agent name=\"{name}\">{emoji} {desc}</agent>\n",
                    name = escape_xml(&a.name)
                ));
            }

            out.push_str("  </domain>\n");
        }

        out.push_str("</available_agents>\n");
        out.push_str("\nSelect EXACT agent names from the catalog above. Do not invent or modify names.\n");
        out
    }

    // ── Progressive Disclosure: Level 2 — Full Profile for Worker ──

    /// Get a full agent profile by EXACT name.
    /// Returns None if the name doesn't exactly match — no fuzzy matching.
    /// This is progressive disclosure Level 2: the full personality document.
    pub fn get(&self, name: &str) -> Option<&AgentProfile> {
        self.by_name.get(name).map(|&idx| &self.agents[idx])
    }

    /// Load agent system prompt for Worker use — Level 2 disclosure.
    pub fn load_for_worker(&self, name: &str) -> Option<String> {
        self.get(name).map(|a| a.system_prompt.clone())
    }

    /// Resolve exact agent names from planner output.
    /// Returns (found_profiles, not_found_names) for diagnostics.
    pub fn resolve_exact(&self, names: &[String]) -> (Vec<&AgentProfile>, Vec<String>) {
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for name in names {
            let trimmed = name.trim();
            if let Some(profile) = self.get(trimmed) {
                found.push(profile);
            } else {
                missing.push(trimmed.to_string());
            }
        }
        (found, missing)
    }

    // ── Query APIs ──

    /// List all agents in a domain.
    pub fn list_by_domain(&self, domain: &str) -> Vec<&AgentProfile> {
        self.by_domain.get(domain)
            .map(|indices| indices.iter().map(|&i| &self.agents[i]).collect())
            .unwrap_or_default()
    }

    /// List all domain names.
    pub fn domains(&self) -> Vec<&str> {
        let mut d: Vec<&str> = self.by_domain.keys().map(|s| s.as_str()).collect();
        d.sort();
        d
    }

    /// Count agents per domain (for display).
    pub fn domain_counts(&self) -> Vec<(&str, usize)> {
        let mut counts: Vec<(&str, usize)> = self.by_domain.iter()
            .map(|(k, v)| (k.as_str(), v.len()))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts
    }

    /// Search agents by keywords (for interactive exploration, NOT for planner).
    pub fn search(&self, query: &str) -> Vec<&AgentProfile> {
        let q = query.to_lowercase();
        self.agents.iter()
            .filter(|a| {
                a.name.to_lowercase().contains(&q)
                    || a.description.to_lowercase().contains(&q)
                    || a.domain.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn empty() -> Self {
        Self { agents: vec![], by_domain: HashMap::new(), by_name: HashMap::new() }
    }

    pub fn len(&self) -> usize { self.agents.len() }
    pub fn is_empty(&self) -> bool { self.agents.is_empty() }
}

// ── YAML Frontmatter Parser ──

fn parse_frontmatter(content: &str) -> (HashMap<String, String>, &str) {
    let mut map = HashMap::new();
    let body_start = if content.starts_with("---\n") || content.starts_with("---\r\n") {
        let after_first = &content[4..];
        if let Some(end) = after_first.find("\n---") {
            let fm = &after_first[..end];
            parse_yaml_kv(fm, &mut map);
            if end + 4 < after_first.len() { end + 5 } else { content.len() }
        } else { 0 }
    } else { 0 };
    let body = &content[body_start..];
    (map, body.trim())
}

fn parse_yaml_kv(yaml: &str, map: &mut HashMap<String, String>) {
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        if let Some(idx) = trimmed.find(':') {
            let key = trimmed[..idx].trim().to_string();
            let value = trimmed[idx + 1..].trim().to_string();
            map.insert(key, value);
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { s.to_string() }
    else { format!("{}...", &s[..max_len]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: Test Agent\ndescription: A test agent\n---\n\n## Body content";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("name").unwrap(), "Test Agent");
        assert_eq!(fm.get("description").unwrap(), "A test agent");
        assert!(body.contains("## Body content"));
    }

    #[test]
    fn test_empty_registry() {
        let r = AgentRegistry::empty();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn test_resolve_exact() {
        let r = AgentRegistry::empty();
        let (found, missing) = r.resolve_exact(&["Backend Architect".into(), "Ghost Agent".into()]);
        assert!(found.is_empty());
        assert_eq!(missing, vec!["Backend Architect", "Ghost Agent"]);
    }
}
