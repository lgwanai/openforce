use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ================================================================
// Expert library types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct ExpertIndex {
    pub version: String,
    pub categories: HashMap<String, Category>,
    pub model_tiers: HashMap<String, ModelTier>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Category {
    pub keywords: Vec<String>,
    pub default_role: String,
    pub sop: Option<String>,
    pub profiles: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ModelTier {
    pub models: Vec<String>,
    #[serde(rename = "for")]
    pub for_desc: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoleProfile {
    pub name: String,
    pub version: u32,
    pub display_name: String,
    pub description: String,
    pub model_tier: String,
    pub default_model: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub allowed_paths: PathPermissions,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathPermissions {
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub forbidden: Vec<String>,
}

// ================================================================
// Knowledge base loader
// ================================================================

pub struct KnowledgeBase {
    pub index: ExpertIndex,
    pub profiles: HashMap<String, RoleProfile>,
    pub base_dir: String,
}

impl KnowledgeBase {
    pub fn load(base_dir: &str) -> Result<Self> {
        let index_path = Path::new(base_dir).join("index.json");
        let index: ExpertIndex = serde_json::from_str(&fs::read_to_string(&index_path)?)?;

        let mut profiles = HashMap::new();
        let profiles_dir = Path::new(base_dir).join("profiles");
        if profiles_dir.exists() {
            for entry in fs::read_dir(&profiles_dir)? {
                let entry = entry?;
                if entry.path().extension().map_or(false, |e| e == "json") {
                    if let Ok(profile) = serde_json::from_str::<RoleProfile>(
                        &fs::read_to_string(entry.path())?
                    ) {
                        profiles.insert(profile.name.clone(), profile);
                    }
                }
            }
        }

        Ok(Self { index, profiles, base_dir: base_dir.to_string() })
    }

    /// Get all profiles for a list of role names
    pub fn get_profiles(&self, roles: &[String]) -> Vec<RoleProfile> {
        roles.iter()
            .filter_map(|name| self.profiles.get(name).cloned())
            .collect()
    }

    /// Get all category names for the LLM to use in classification
    pub fn category_summary(&self) -> String {
        self.index.categories.iter()
            .map(|(name, cat)| {
                format!("- {name}: {} (roles: {})",
                    cat.keywords.iter().take(3).map(|k| k.as_str()).collect::<Vec<_>>().join(", "),
                    cat.profiles.iter().take(3).map(|p| p.as_str()).collect::<Vec<_>>().join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ================================================================
// Semantic task classification (LLM-powered, NOT keyword matching)
// ================================================================

#[derive(Debug, Deserialize)]
pub struct ClassificationResult {
    pub categories: Vec<String>,
    pub suggested_roles: Vec<String>,
    pub task_type: String,
    pub complexity: String,
}

/// Use LLM to semantically understand the task and classify it into categories.
/// This replaces brittle keyword matching with true semantic understanding.
pub async fn semantic_classify(
    llm: &LlmClient,
    task: &str,
    kb: &KnowledgeBase,
) -> Result<ClassificationResult> {
    let categories = kb.category_summary();

    let prompt = format!(
        "你是一个任务分类器。分析用户的任务描述，将其归类到最匹配的类别中。\n\n\
         可用类别:\n{categories}\n\n\
         用户任务: {task}\n\n\
         请输出 JSON，不要输出其他内容:\n\
         {{\n\
           \"categories\": [\"最匹配的类别名1\", \"类别名2\"],\n\
           \"suggested_roles\": [\"推荐角色名1\", \"角色名2\"],\n\
           \"task_type\": \"简短的任务类型描述\",\n\
           \"complexity\": \"simple|medium|complex\"\n\
         }}\n\n\
         注意:\n\
         - categories 必须是上面列出的类别名\n\
         - suggested_roles 必须是上面列出的角色名\n\
         - 深入理解任务语义，不要仅做关键词匹配。例如\"图书管理系统\"应该匹配backend+frontend，不是只匹配包含\"图书\"的类别"
    );

    let system = "你是一个精确的任务分类器。只输出 JSON，不要解释。";
    let (response, _) = llm.chat(system, &prompt).await?;

    // Extract JSON from response (LLM may wrap it in markdown)
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else { &response }
    } else { return Err(anyhow::anyhow!("No JSON in LLM response: {response:.200}")) };

    let classification: ClassificationResult = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse classification JSON: {e}\nResponse: {json_str:.200}"))?;

    Ok(classification)
}
