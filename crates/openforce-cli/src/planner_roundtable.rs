use openforce_llm_client::LlmClient;
use openforce_knowledge_base::ClassificationResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Task Tree & Plan Types ──

#[derive(Debug, Clone)]
pub struct TaskTree {
    pub goal: SmartGoal,
    pub data_sources: Vec<String>,
    pub tasks: Vec<DecomposedTask>,
    pub mece_validated: bool,
    pub confidence: String,
    pub needs_info: Vec<String>,
    pub plan_steps: Vec<PlanStep>,
}

#[derive(Debug, Clone)]
pub struct SmartGoal {
    pub specific: String, pub measurable: String, pub achievable: String,
    pub relevant: String, pub time_bound: String,
}

#[derive(Debug, Clone)]
pub struct DecomposedTask {
    pub role: String, pub title: String, pub objective: String,
    pub files: Vec<String>, pub steps: Vec<String>,
    pub acceptance_criteria: Vec<String>, pub dependencies: Vec<String>,
    pub priority: String,
    pub estimated_cycles: usize,
}

#[derive(Debug, Clone)]
pub struct PlanStep {
    pub phase: String,
    pub description: String,
    pub tasks: Vec<String>,
}

pub const PLAN_MODE_HEADER: &str = "\
## Plan Mode — CRITICAL
You are in PLAN MODE (READ ONLY). You CANNOT write, edit, delete files, or run shell commands.
Your ONLY responsibility is to THINK, READ, and PLAN.
When ready, present your plan for user approval.
";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoQuestion {
    pub question: String,
    pub category: String,
    pub options: Vec<String>,
}

// ── Dynamic Analysis Dimensions ──

fn analysis_dimensions(categories: &[String]) -> Vec<(&'static str, &'static str)> {
    let mut dims = Vec::new();
    dims.push(("architecture", "System architecture & module boundaries"));
    dims.push(("implementation", "Implementation feasibility & approach"));

    for cat in categories {
        match cat.as_str() {
            "frontend" | "ui" | "web" => {
                dims.push(("ux_interaction", "User experience & interaction flow"));
                dims.push(("component_design", "Component hierarchy & state management"));
            }
            "backend" | "api" | "service" => {
                dims.push(("data_model", "Data model, schema & persistence"));
                dims.push(("api_design", "API contract, versioning & error handling"));
            }
            "database" | "storage" => {
                dims.push(("data_integrity", "Data integrity, migrations & consistency"));
                dims.push(("query_performance", "Query patterns & performance"));
            }
            "security" | "auth" => {
                dims.push(("security_review", "Authentication, authorization & threat model"));
                dims.push(("compliance", "Compliance requirements & audit trail"));
            }
            "devops" | "deployment" | "infra" => {
                dims.push(("deployment", "Deployment strategy & environment"));
                dims.push(("monitoring", "Observability, logging & alerting"));
            }
            "testing" | "qa" => {
                dims.push(("test_strategy", "Test coverage, E2E flows & edge cases"));
            }
            "performance" | "optimization" => {
                dims.push(("perf_analysis", "Performance bottlenecks & profiling"));
            }
            _ => {
                if dims.len() < 6 { dims.push(("risk_assessment", "Risk identification & mitigation")); }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    dims.retain(|(k, _)| seen.insert(k.to_string()));
    dims.truncate(6);
    dims
}

// ── RoundTable with Dynamic Dimensions ──

pub async fn run_roundtable(
    planner: &LlmClient, task: &str,
    classification: &ClassificationResult,
    available_roles: &[String], agent_catalog: &str,
    skills_dir: &str, dir_summary: &str,
) -> Result<TaskTree, String> {
    let roles_str = available_roles.join(", ");
    let skill_runner = crate::skill_runner::SkillRunner::discover(skills_dir);
    let skill_summary = if skill_runner.has_skills() {
        format!("Available Skills:\n{}", skill_runner.skill_summary())
    } else { String::new() };
    let mut needs_info: Vec<String> = vec![];

    let dimensions = analysis_dimensions(&classification.categories);
    eprintln!("  Analysis dimensions: {}", dimensions.len());

    // ── Round 1: Multi-agent proposals from different dimensions ──
    let mut proposals: HashMap<String, String> = HashMap::new();

    // Convert to owned data to avoid lifetime issues with tokio::spawn
    let owned_dims: Vec<(String, String)> = dimensions.iter()
        .map(|(a, b)| (a.to_string(), b.to_string())).collect();

    for chunk in owned_dims.chunks(3) {
        let mut futures = Vec::new();
        for (dim_key, dim_desc) in chunk.to_vec() {
            let base = format!(
                "你是{dim_desc}专家。\n\n任务: {task}\n{agents}\n项目结构:\n{dir}\n{skills}\n\n\
                 从{dim_desc}维度 MECE 分解。同一角色可有多个并行Worker（只要互不干扰）。子任务从 <available_agents> 指定 EXACT agent name。标注: [需询问用户: ...] [需网络检索: ...]。",
                dim_desc = dim_desc, agents = agent_catalog, dir = dir_summary, skills = skill_summary
            );
            let client = planner.clone();
            let key = dim_key.clone();
            let dk = dim_key;
            let dd = dim_desc;
            futures.push(tokio::spawn(async move {
                match client.chat(&format!("你是{dd}专家。"), &base).await {
                    Ok((text, _)) => (key, text),
                    Err(e) => (key, format!("ERROR: {e}")),
                }
            }));
        }
        for f in futures {
            if let Ok((k, v)) = f.await { proposals.insert(k, v); }
        }
    }

    if proposals.is_empty() { return Err("RoundTable: all proposals failed".into()); }

    // Collect information needs
    for text in proposals.values() {
        for line in text.lines() {
            let t = line.trim();
            if t.contains("[需询问用户:") || t.contains("[需网络检索:") || t.contains("[QUESTION:") {
                needs_info.push(t.to_string());
            }
        }
    }

    // Execute web searches
    let mut search_results = String::new();
    for info in &needs_info {
        if info.contains("[需网络检索:") {
            if let Some(q) = info.split("[需网络检索:").nth(1).and_then(|s| s.split(']').next()) {
                let result = skill_runner.resolve_search(q).await;
                search_results.push_str(&format!("\nSearch '{q}':\n{result}\n"));
            }
        }
    }

    // ── Round 2: Cross-Review ──
    let proposals_text: String = proposals.iter()
        .map(|(k, v)| format!("\n=== {k} ===\n{v}\n")).collect::<Vec<_>>().join("\n");

    let review_prompt = format!(
        "你是项目审查员。\n{agents}\n\n原始任务: {task}\n\n各维度分析:\n{pr}\n\n信息缺口:\n{g}\n\n检索结果:\n{s}\n\n\
         交叉审查: MECE检查(重叠?遗漏?), 确认同角色任务是否可并行, 优先级评估(high/medium/low), 输出最优合并列表。\n\
         每个子任务必须从 <available_agents> 中指定 EXACT agent name。格式: [Agent Name] 标题: 描述 | 优先级 | 依赖:[...] | 验收:[...]",
        agents = agent_catalog,
        pr = proposals_text,
        g = needs_info.iter().take(10).map(|s| s.as_str()).collect::<Vec<_>>().join("\n"),
        s = search_results
    );

    let (review_text, _) = planner.chat("你是资深审查员。交叉验证各维度方案。", &review_prompt)
        .await.map_err(|e| format!("cross-review: {e}"))?;

    // ── Round 3: Final Synthesis ──
    let synth_prompt = format!(
        "你是最终决策者。\n{agents}\n\n任务: {task}\n审查结果:\n{review}\n\n输出JSON(仅JSON):\n\
         {{\"goal\":{{\"specific\":\"...\",\"measurable\":\"...\",\"achievable\":\"...\",\"relevant\":\"...\",\"time_bound\":\"...\"}},\
         \"data_sources\":[\"路径\"],\
         \"tasks\":[{{\"role\":\"从 <available_agents> 中选EXACT name\",\"title\":\"标题\",\"objective\":\"目标\",\
         \"files\":[\"文件\"],\"steps\":[\"步骤\"],\"acceptance_criteria\":[\"验收\"],\
         \"dependencies\":[\"其他任务title\"],\"priority\":\"high|medium|low\",\"estimated_cycles\":数字}}],\
         \"mece_validated\":true,\"confidence\":\"high|medium|low\",\
         \"plan_steps\":[{{\"phase\":\"阶段\",\"description\":\"描述\",\"tasks\":[\"任务title\"]}}]}}",
        agents = agent_catalog, review = review_text
    );

    let (final_json, _) = planner.chat("你是决策者。仅输出JSON。", &synth_prompt)
        .await.map_err(|e| format!("synthesize: {e}"))?;

    let mut tree = parse_task_tree(&final_json)?;
    tree.needs_info = needs_info;
    Ok(tree)
}

// ── Interactive Info Gathering ──

pub fn extract_questions(needs_info: &[String]) -> Vec<InfoQuestion> {
    let mut questions = Vec::new();
    for info in needs_info {
        let t = info.trim();
        if t.contains("[需询问用户:") {
            if let Some(q) = t.split("[需询问用户:").nth(1).and_then(|s| s.split(']').next()) {
                questions.push(InfoQuestion { question: q.trim().to_string(), category: "user_intent".into(), options: vec![] });
            }
        }
        if t.contains("[需确认:") || t.contains("[QUESTION:") {
            let q_text = t.split("[需确认:").nth(1).or_else(|| t.split("[QUESTION:").nth(1))
                .and_then(|s| s.split(']').next()).unwrap_or("").trim();
            if !q_text.is_empty() {
                questions.push(InfoQuestion { question: q_text.to_string(), category: "architecture".into(), options: vec![] });
            }
        }
    }
    questions
}

// ── Plan Mode Enforcement ──

pub fn check_plan_mode_tool(tool_name: &str, args: &str) -> Option<String> {
    let readonly_tools = &["read_file", "grep", "glob", "web_search", "fetch", "list_files", "read", "find"];
    if readonly_tools.iter().any(|t| tool_name.contains(t)) { return None; }

    let destructive = &[
        "rm ", "mv ", "cp ", "mkdir", "touch", "npm install", "pip install",
        "git add", "git commit", "git push", "sudo ", "kill ", "chmod",
        "delete", "remove", "drop ", "truncate",
    ];
    for p in destructive {
        if tool_name.contains(p) || args.to_lowercase().contains(p) {
            return Some(format!("Plan Mode: '{tool_name}' not allowed (read-only)"));
        }
    }

    if tool_name == "shell_exec" || tool_name == "bash" {
        let safe = &["cat ", "head ", "tail ", "grep ", "find ", "ls ", "pwd ", "wc ",
            "sort ", "uniq ", "diff ", "file ", "which ", "git status", "git log", "git diff"];
        if safe.iter().any(|p| args.trim().starts_with(p)) { return None; }
        return Some(format!("Plan Mode: shell may modify state"));
    }

    Some(format!("Plan Mode: '{tool_name}' not in read-only allowlist"))
}

// ── JSON Parsing ──

fn parse_task_tree(json_str: &str) -> Result<TaskTree, String> {
    let json = json_str.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    #[derive(Deserialize)] struct R { goal: G, #[serde(default)] data_sources: Vec<String>, tasks: Vec<T>, #[serde(default)] mece_validated: bool, #[serde(default)] confidence: String, #[serde(default)] plan_steps: Vec<S> }
    #[derive(Deserialize)] struct G { #[serde(default)] specific: String, #[serde(default)] measurable: String, #[serde(default)] achievable: String, #[serde(default)] relevant: String, #[serde(default)] time_bound: String }
    #[derive(Deserialize)] struct T { #[serde(default)] role: String, #[serde(default)] title: String, #[serde(default)] objective: String, #[serde(default)] files: Vec<String>, #[serde(default)] steps: Vec<String>, #[serde(default)] acceptance_criteria: Vec<String>, #[serde(default)] dependencies: Vec<String>, #[serde(default)] priority: String, #[serde(default)] estimated_cycles: usize }
    #[derive(Deserialize)] struct S { #[serde(default)] phase: String, #[serde(default)] description: String, #[serde(default)] tasks: Vec<String> }
    let r: R = serde_json::from_str(json).map_err(|e| format!("parse: {e} | JSON: {json:.300}"))?;
    Ok(TaskTree {
        goal: SmartGoal { specific: r.goal.specific, measurable: r.goal.measurable, achievable: r.goal.achievable, relevant: r.goal.relevant, time_bound: r.goal.time_bound },
        data_sources: r.data_sources,
        tasks: r.tasks.into_iter().map(|t| DecomposedTask {
            role: t.role, title: t.title, objective: t.objective, files: t.files, steps: t.steps,
            acceptance_criteria: t.acceptance_criteria, dependencies: t.dependencies,
            priority: if t.priority.is_empty() { "medium".into() } else { t.priority },
            estimated_cycles: if t.estimated_cycles == 0 { 5 } else { t.estimated_cycles },
        }).collect(),
        mece_validated: r.mece_validated,
        confidence: if r.confidence.is_empty() { "medium".into() } else { r.confidence },
        needs_info: vec![],
        plan_steps: r.plan_steps.into_iter().map(|s| PlanStep { phase: s.phase, description: s.description, tasks: s.tasks }).collect(),
    })
}
