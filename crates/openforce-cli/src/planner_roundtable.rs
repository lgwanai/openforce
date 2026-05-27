use openforce_llm_client::LlmClient;
use openforce_knowledge_base::ClassificationResult;
use openforce_domain::session_phase::{SessionPhase, PhaseGroup};
use openforce_domain::worker_folder::{WorkerOutputFolder, WorkerStatus, OutputRef};
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
    pub sketched: bool,
}

#[derive(Debug, Clone)]
pub struct PlanStep {
    pub phase: String,
    pub description: String,
    pub tasks: Vec<String>,
    pub phase_group: String,
}

#[allow(dead_code)]
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

// ── Dynamic Analysis Dimensions (Phase-Group-Aware) ──

fn analysis_dimensions(categories: &[String], phase_group: PhaseGroup) -> Vec<(&'static str, &'static str)> {
    let mut dims = Vec::new();
    match phase_group {
        PhaseGroup::Design => {
            dims.push(("architecture", "System architecture & module boundaries"));
            dims.push(("data_model", "Data model, schema & persistence"));
            for cat in categories {
                match cat.as_str() {
                    "frontend" | "ui" | "web" => {
                        dims.push(("ux_interaction", "User experience & interaction flow"));
                        dims.push(("component_design", "Component hierarchy & state management"));
                    }
                    "backend" | "api" | "service" => {
                        dims.push(("api_design", "API contract, versioning & error handling"));
                    }
                    "database" | "storage" => {
                        dims.push(("data_integrity", "Data integrity, migrations & consistency"));
                    }
                    "security" | "auth" => {
                        dims.push(("security_review", "Authentication, authorization & threat model"));
                    }
                    _ => {}
                }
            }
        }
        PhaseGroup::Implementation => {
            dims.push(("implementation", "Implementation approach & coding strategy"));
            dims.push(("test_strategy", "Test coverage, E2E flows & edge cases"));
            for cat in categories {
                match cat.as_str() {
                    "frontend" | "ui" | "web" => {
                        dims.push(("component_impl", "Component implementation & state wiring"));
                    }
                    "backend" | "api" | "service" => {
                        dims.push(("endpoint_impl", "Endpoint implementation & error handling"));
                    }
                    "database" | "storage" => {
                        dims.push(("data_integrity", "Data integrity, migrations & consistency"));
                        dims.push(("query_performance", "Query patterns & performance"));
                    }
                    "security" | "auth" => {
                        dims.push(("compliance", "Compliance requirements & audit trail"));
                    }
                    "devops" | "deployment" | "infra" => {
                        dims.push(("deployment", "Deployment strategy & environment"));
                        dims.push(("monitoring", "Observability, logging & alerting"));
                    }
                    "performance" | "optimization" => {
                        dims.push(("perf_analysis", "Performance bottlenecks & profiling"));
                    }
                    _ => {}
                }
            }
        }
        PhaseGroup::Report => {
            dims.push(("report", "Summary & documentation"));
        }
    }
    if dims.len() < 2 {
        dims.push(("risk_assessment", "Risk identification & mitigation"));
    }
    let mut seen = std::collections::HashSet::new();
    dims.retain(|(k, _)| seen.insert(k.to_string()));
    dims.truncate(6);
    dims
}

// ── Worker Output Folder: LLM-Summarized ──
//  Session IS the folder. Planner only sees LLM-summarized key info.

#[derive(Debug, Deserialize)]
struct FolderSummary {
    #[serde(default)] one_line_summary: String,
    #[serde(default)] key_findings: Vec<String>,
}

async fn summarize_with_llm(
    llm: &LlmClient, worker_id: &str, role: &str, title: &str,
    task_objective: Option<&str>, output_text: &str, success: bool,
) -> Result<FolderSummary, String> {
    let success_str = if success { "success" } else { "failed" };
    let objective = task_objective.unwrap_or("(unspecified)");
    let prompt = format!(
        "<worker_info>\nID: {worker_id}\nRole: {role}\nTitle: {title}\n\
         Objective: {objective}\nStatus: {success_str}\n</worker_info>\n\n\
         <full_output>\n{output_text}\n</full_output>\n\n\
         Extract:\n\
         1. one_line_summary: 1 sentence (≤120 chars) summarizing the core result\n\
         2. key_findings: key findings/decisions, max 5 items. Focus on:\
         surprises, decisions made, problems found, outputs produced\n\
         3. matches_objective: whether output aligns with assigned objective (true/false)\n\n\
         JSON only: {{\"one_line_summary\":\"...\",\"key_findings\":[\"...\"],\"matches_objective\":bool}}"
    );
    let (json, _) = llm.chat("You are a result summarizer. Extract key information from worker output. JSON only.", &prompt).await
        .map_err(|e| format!("summarize llm: {e}"))?;
    let cleaned = json.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    serde_json::from_str(cleaned).map_err(|e| format!("parse summary: {e} | JSON: {cleaned:.200}"))
}

fn read_folder_metadata(output_path: &str) -> (usize, usize, Vec<String>, String) {
    let (mut done, mut total, mut findings, mut output) = (0, 0, vec![], String::new());
    if let Ok(s) = std::fs::read_to_string(output_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            done = v["subtasks_completed"].as_u64().unwrap_or(0) as usize;
            total = v["subtasks_total"].as_u64().unwrap_or(0) as usize;
            findings = v["memory_snapshot"]["key_findings"].as_array()
                .map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).take(5).collect())
                .unwrap_or_default();
            output = v["output"].as_str().unwrap_or("").to_string();
        }
    }
    (done, total, findings, output)
}

fn fallback_one_liner(text: &str, done: usize, total: usize) -> String {
    text.lines().map(|l| l.trim())
        .filter(|l| !l.is_empty() && l.len() > 5
            && !l.starts_with('#') && !l.starts_with("---")
            && !l.starts_with("===") && !l.starts_with("Error:")
            && !l.starts_with("|") && !l.starts_with("```"))
        .next()
        .map(|l| truncate_str(l, 120))
        .unwrap_or_else(|| format!("Completed {done}/{total} subtasks"))
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let end = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}...", &s[..end])
}

pub async fn summarize_worker_output_from_file(
    llm: &LlmClient, worker_id: &str, role: &str, title: &str,
    task_objective: Option<&str>, output_path: &str,
    success: bool, action: &str, cycles: usize,
) -> WorkerOutputFolder {
    let (done, total, file_findings, output_text) = read_folder_metadata(output_path);
    let (one_liner, key_notes) = match summarize_with_llm(
        llm, worker_id, role, title, task_objective, &output_text, success,
    ).await {
        Ok(s) => {
            let line = if s.one_line_summary.is_empty() {
                fallback_one_liner(&output_text, done, total)
            } else { truncate_str(&s.one_line_summary, 120) };
            let notes = if s.key_findings.is_empty() { file_findings } else { s.key_findings };
            (line, notes)
        }
        Err(e) => {
            eprintln!("  [Folder] LLM summary failed for {worker_id}: {e} — using fallback");
            (fallback_one_liner(&output_text, done, total), file_findings)
        }
    };
    WorkerOutputFolder {
        worker_id: worker_id.to_string(), role: role.to_string(), title: title.to_string(),
        status: WorkerStatus::from_status_and_action(success, action),
        one_liner, key_notes,
        output_ref: OutputRef::file(output_path),
        subtasks_done: done, subtasks_total: total, cycles,
    }
}

pub async fn summarize_worker_output_from_text(
    llm: &LlmClient, worker_id: &str, role: &str, title: &str,
    task_objective: Option<&str>, output_text: &str,
    success: bool, action: &str, cycles: usize,
) -> WorkerOutputFolder {
    let (one_liner, key_notes) = match summarize_with_llm(
        llm, worker_id, role, title, task_objective, output_text, success,
    ).await {
        Ok(s) => {
            let line = if s.one_line_summary.is_empty() {
                fallback_one_liner(output_text, 0, 0)
            } else { truncate_str(&s.one_line_summary, 120) };
            (line, s.key_findings)
        }
        Err(e) => {
            eprintln!("  [Folder] LLM summary failed for {worker_id}: {e} — using fallback");
            (fallback_one_liner(output_text, 0, 0), vec![])
        }
    };
    WorkerOutputFolder {
        worker_id: worker_id.to_string(), role: role.to_string(), title: title.to_string(),
        status: WorkerStatus::from_status_and_action(success, action),
        one_liner, key_notes,
        output_ref: OutputRef::file(""),
        subtasks_done: 0, subtasks_total: 0, cycles,
    }
}

#[allow(dead_code)]
pub async fn expand_folder(
    folder: &WorkerOutputFolder,
    redis_store: Option<&mut openforce_redis_session::store::RedisSessionStore>,
) -> Result<String, String> {
    match &folder.output_ref {
        OutputRef::File(path) if !path.is_empty() => {
            let json_str = std::fs::read_to_string(path)
                .map_err(|e| format!("read file {path}: {e}"))?;
            let v: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| format!("parse json: {e}"))?;
            Ok(v["output"].as_str().unwrap_or("(no output)").to_string())
        }
        OutputRef::Redis { session_id, worker_id } => match redis_store {
            Some(store) => {
                let sid = uuid::Uuid::parse_str(session_id)
                    .map_err(|e| format!("parse session_id: {e}"))?;
                match store.get_worker_snapshot(&sid, worker_id).await? {
                    Some(sn) => Ok(sn.outputs.join("\n")),
                    None => Err("worker snapshot not found".into()),
                }
            }
            None => Err("Redis store not available".into()),
        },
        _ => Err("no output reference available".into()),
    }
}

// ── RoundTable: Phase-Aware Progressive Planning ──

pub async fn pre_plan_clarify(
    planner: &LlmClient, task: &str,
) -> Result<Vec<InfoQuestion>, String> {
    let prompt = format!(
        "分析以下任务，判断信息是否充足。如果缺少关键信息导致无法准确规划，列出需要用户澄清的问题。\n\n任务: {task}\n\n\
         输出JSON: {{\"sufficient\":true|false,\"questions\":[{{\"question\":\"...\",\"options\":[\"A\",\"B\"]}}]}}\n\
         如果信息充足，返回 sufficient:true, questions:[]。仅JSON。"
    );
    let (json, _) = planner.chat("你是需求分析师。判断信息是否充足。", &prompt).await
        .map_err(|e| format!("clarify: {e}"))?;
    let cleaned = json.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    #[derive(Deserialize)] struct C { #[serde(default)] sufficient: bool, #[serde(default)] questions: Vec<InfoQuestion> }
    let c: C = serde_json::from_str(cleaned).unwrap_or(C { sufficient: true, questions: vec![] });
    Ok(if c.sufficient { vec![] } else { c.questions })
}

pub async fn replan_failed(
    planner: &LlmClient, original_task: &str,
    failed_summary: &str, agent_catalog: &str,
    skills_dir: &str, dir_summary: &str,
    folders: &[WorkerOutputFolder],
) -> Result<TaskTree, String> {
    let skill_runner = crate::skill_runner::SkillRunner::discover(skills_dir);
    let folder_context = WorkerOutputFolder::format_for_prompt(folders);
    let prompt = format!(
        "你是 Planner。之前的执行计划部分失败，需要重新规划。\n\n原始任务: {task}\n{agents}\n项目结构:\n{dir}\n{skills}\n\n\
         {folders}\
         失败原因汇总:\n{failed}\n\n\
         分析根因，重新分解失败的任务。输出JSON:\n\
         {{\"goal\":{{...}},\"tasks\":[{{\"role\":\"EXACT agent name\",\"title\":\"...\",\
         \"objective\":\"...\",\"acceptance_criteria\":[\"可量化指标\"],\
         \"dependencies\":[\"其他任务title\"],\"priority\":\"high|medium|low\"}}],\
         \"mece_validated\":true,\"confidence\":\"high|medium|low\"}}",
        task=original_task, agents=agent_catalog, dir=dir_summary,
        skills=skill_runner.skill_summary(), failed=failed_summary,
        folders=folder_context,
    );
    let (json, _) = planner.chat("你是 Planner。分析失败原因并重新规划。仅输出JSON。", &prompt).await
        .map_err(|e| format!("replan: {e}"))?;
    parse_task_tree(&json)
}

#[allow(dead_code)]
pub async fn run_roundtable(
    planner: &LlmClient, task: &str,
    classification: &ClassificationResult,
    _available_roles: &[String], agent_catalog: &str,
    skills_dir: &str, dir_summary: &str,
) -> Result<TaskTree, String> {
    run_roundtable_phased(planner, task, classification, _available_roles, agent_catalog, skills_dir, dir_summary, None, &[]).await
}

pub async fn run_roundtable_phased(
    planner: &LlmClient, task: &str,
    classification: &ClassificationResult,
    _available_roles: &[String], agent_catalog: &str,
    skills_dir: &str, dir_summary: &str,
    current_phase: Option<&SessionPhase>,
    previous_folders: &[WorkerOutputFolder],
) -> Result<TaskTree, String> {
    let skill_runner = crate::skill_runner::SkillRunner::discover(skills_dir);
    let skill_summary = if skill_runner.has_skills() {
        format!("Available Skills:\n{}", skill_runner.skill_summary())
    } else { String::new() };
    let previous_context = WorkerOutputFolder::format_for_prompt(previous_folders);
    let mut needs_info: Vec<String> = vec![];

    let pg = current_phase.map(|p| p.phase_group());
    let dimensions = analysis_dimensions(&classification.categories, pg.unwrap_or(PhaseGroup::Design));
    eprintln!("  Phase group: {:?}, dimensions: {}", pg.unwrap_or(PhaseGroup::Design).as_str(), dimensions.len());

    let phase_instruction = match pg {
        Some(PhaseGroup::Design) => format!(
            "## PLANNING MODE: DESIGN PHASE ONLY\n\
             You are planning the DESIGN/ARCHITECTURE phase. \n\
             - ONLY produce design tasks: architecture, data model, API contract, UI spec, component design, tech decisions.\n\
             - DO NOT produce implementation/coding/testing tasks. They will be planned in a later phase.\n\
             - DO NOT use sketched=true. All tasks in this phase are concrete.\n\
             - Dependencies: backend architecture > data model > API design > frontend architecture > UI spec.\n\
             - Each task must have clear dependencies on earlier tasks."
        ),
        Some(PhaseGroup::Implementation) => format!(
            "## PLANNING MODE: IMPLEMENTATION PHASE\n\
             Design is COMPLETE. Plan all implementation tasks based on the design output.\n\
             - Produce concrete implementation/coding/testing tasks.\n\
             - All tasks sketched: false.\n\
             - Dependencies must reference completed design tasks."
        ),
        Some(PhaseGroup::Report) => format!(
            "## PLANNING MODE: REPORT PHASE\n\
             Produce report, documentation, summary tasks. All concrete."
        ),
        None => String::new(),
    };

    let mut proposals: HashMap<String, String> = HashMap::new();
    let owned_dims: Vec<(String, String)> = dimensions.iter()
        .map(|(a, b)| (a.to_string(), b.to_string())).collect();

    eprintln!("  Round 1/3: {} analysis dimensions in {} parallel chunks...", owned_dims.len(), (owned_dims.len() + 2) / 3);
    for chunk in owned_dims.chunks(3) {
        let mut futures = Vec::new();
        for (dim_key, dim_desc) in chunk.to_vec() {
            let base = format!(
                "{phase_instruction}\n\n{previous}\n你是{dim_desc}专家。\n\n任务: {task}\n{agents}\n项目结构:\n{dir}\n{skills}\n\n\
                 从{dim_desc}维度 MECE 分解。同一角色可有多个并行Worker。子任务从 <available_agents> 指定 EXACT agent name。标注: [需询问用户: ...] [需网络检索: ...]。",
                phase_instruction = phase_instruction, previous = previous_context,
                dim_desc = dim_desc, agents = agent_catalog, dir = dir_summary, skills = skill_summary
            );
            let client = planner.clone();
            let key = dim_key.clone();
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

    for text in proposals.values() {
        for line in text.lines() {
            let t = line.trim();
            if t.contains("[需询问用户:") || t.contains("[需网络检索:") || t.contains("[QUESTION:") {
                needs_info.push(t.to_string());
            }
        }
    }

    let mut search_results = String::new();
    for info in &needs_info {
        if info.contains("[需网络检索:") {
            if let Some(q) = info.split("[需网络检索:").nth(1).and_then(|s| s.split(']').next()) {
                let result = skill_runner.resolve_search(q).await;
                search_results.push_str(&format!("\nSearch '{q}':\n{result}\n"));
            }
        }
    }

    let proposals_text: String = proposals.iter()
        .map(|(k, v)| format!("\n=== {k} ===\n{v}\n")).collect::<Vec<_>>().join("\n");

    eprintln!("  Round 2/3: Cross-review...");
    let review_prompt = format!(
        "{phase_instruction}\n\n{previous}\n你是项目审查员。\n{agents}\n\n原始任务: {task}\n\n各维度分析:\n{pr}\n\n信息缺口:\n{g}\n\n检索结果:\n{s}\n\n\
         交叉审查: MECE检查, 确认同角色任务是否可并行, 优先级评估, 输出最优合并列表。\n\
         每个子任务必须从 <available_agents> 中指定 EXACT agent name。格式: [Agent Name] 标题: 描述 | 优先级 | 依赖:[...] | 验收:[...]",
        phase_instruction = phase_instruction, previous = previous_context,
        agents = agent_catalog, pr = proposals_text,
        g = needs_info.iter().take(10).map(|s| s.as_str()).collect::<Vec<_>>().join("\n"),
        s = search_results
    );
    let (review_text, _) = planner.chat("你是资深审查员。交叉验证各维度方案。", &review_prompt)
        .await.map_err(|e| format!("cross-review: {e}"))?;

    eprintln!("  Round 3/3: Final synthesis...");
    let synth_prompt = format!(
        "{phase_instruction}\n\n你是最终决策者。\n{agents}\n\n任务: {task}\n审查结果:\n{review}\n\n\
         MECE分解: 同一角色可多次出现。每个任务的 acceptance_criteria 必须可量化。\
         \n\n输出JSON(仅JSON):\n\
         {{\"goal\":{{\"specific\":\"...\",\"measurable\":\"...\",\"achievable\":\"...\",\"relevant\":\"...\",\"time_bound\":\"...\"}},\
         \"data_sources\":[\"路径\"],\
         \"tasks\":[{{\"role\":\"EXACT agent name\",\"title\":\"标题\",\"objective\":\"目标\",\
         \"files\":[\"文件\"],\"steps\":[\"步骤\"],\"acceptance_criteria\":[\"可量化指标\"],\
         \"dependencies\":[\"其他任务title\"],\"priority\":\"high|medium|low\",\"estimated_cycles\":数字}}],\
         \"mece_validated\":true,\"confidence\":\"high|medium|low\",\
         \"plan_steps\":[{{\"phase\":\"阶段\",\"description\":\"描述\",\"tasks\":[\"任务title\"],\
         \"phase_group\":\"design|implementation|report\"}}]}}",
        phase_instruction = phase_instruction, agents = agent_catalog, review = review_text
    );
    let (final_json, _) = planner.chat("你是决策者。仅输出JSON。", &synth_prompt)
        .await.map_err(|e| format!("synthesize: {e}"))?;

    let mut tree = parse_task_tree(&final_json)?;
    tree.needs_info = needs_info;

    let has_sketched = tree.tasks.iter().any(|t| t.sketched);
    let has_concrete = tree.tasks.iter().any(|t| !t.sketched);
    match pg {
        Some(PhaseGroup::Design) => {
            if has_concrete && !has_sketched {
                eprintln!("  [Planner] warning: no sketched tasks in design phase");
            }
            if has_sketched && !has_concrete {
                eprintln!("  [Planner] warning: all tasks sketched, none concrete");
            }
        }
        Some(PhaseGroup::Implementation) | Some(PhaseGroup::Report) => {
            if has_sketched {
                eprintln!("  [Planner] warning: forcing sketched=false in concrete phase");
                for t in &mut tree.tasks { t.sketched = false; }
            }
        }
        None => {}
    }

    Ok(tree)
}

// ── Helpers ──

#[allow(dead_code)]
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

#[allow(dead_code)]
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

fn parse_task_tree(json_str: &str) -> Result<TaskTree, String> {
    let json = json_str.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    #[derive(Deserialize)] struct R { goal: G, #[serde(default)] data_sources: Vec<String>, tasks: Vec<T>, #[serde(default)] mece_validated: bool, #[serde(default)] confidence: String, #[serde(default)] plan_steps: Vec<S> }
    #[derive(Deserialize)] struct G { #[serde(default)] specific: String, #[serde(default)] measurable: String, #[serde(default)] achievable: String, #[serde(default)] relevant: String, #[serde(default)] time_bound: String }
    #[derive(Deserialize)] struct T { #[serde(default)] role: String, #[serde(default)] title: String, #[serde(default)] objective: String, #[serde(default)] files: Vec<String>, #[serde(default)] steps: Vec<String>, #[serde(default)] acceptance_criteria: Vec<String>, #[serde(default)] dependencies: Vec<String>, #[serde(default)] priority: String, #[serde(default)] estimated_cycles: usize, #[serde(default)] sketched: bool }
    #[derive(Deserialize)] struct S { #[serde(default)] phase: String, #[serde(default)] description: String, #[serde(default)] tasks: Vec<String>, #[serde(default)] phase_group: String }
    let r: R = serde_json::from_str(json).map_err(|e| format!("parse: {e} | JSON: {json:.300}"))?;
    Ok(TaskTree {
        goal: SmartGoal { specific: r.goal.specific, measurable: r.goal.measurable, achievable: r.goal.achievable, relevant: r.goal.relevant, time_bound: r.goal.time_bound },
        data_sources: r.data_sources,
        tasks: r.tasks.into_iter().map(|t| DecomposedTask {
            role: t.role, title: t.title, objective: t.objective, files: t.files, steps: t.steps,
            acceptance_criteria: t.acceptance_criteria, dependencies: t.dependencies,
            priority: if t.priority.is_empty() { "medium".into() } else { t.priority },
            estimated_cycles: if t.estimated_cycles == 0 { 5 } else { t.estimated_cycles },
            sketched: t.sketched,
        }).collect(),
        mece_validated: r.mece_validated,
        confidence: if r.confidence.is_empty() { "medium".into() } else { r.confidence },
        needs_info: vec![],
        plan_steps: r.plan_steps.into_iter().map(|s| PlanStep { phase: s.phase, description: s.description, tasks: s.tasks, phase_group: s.phase_group }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_dimensions_design_group() {
        let dims = analysis_dimensions(&["backend".into(), "api".into()], PhaseGroup::Design);
        assert!(dims.iter().any(|(k, _)| *k == "architecture"));
        assert!(dims.iter().any(|(k, _)| *k == "data_model"));
        assert!(dims.iter().any(|(k, _)| *k == "api_design"));
        assert!(!dims.iter().any(|(k, _)| *k == "implementation"));
        assert!(!dims.iter().any(|(k, _)| *k == "test_strategy"));
    }

    #[test]
    fn test_analysis_dimensions_design_with_storage() {
        let dims = analysis_dimensions(&["database".into()], PhaseGroup::Design);
        assert!(dims.iter().any(|(k, _)| *k == "data_integrity"));
    }

    #[test]
    fn test_analysis_dimensions_impl_group() {
        let dims = analysis_dimensions(&["backend".into(), "testing".into()], PhaseGroup::Implementation);
        assert!(dims.iter().any(|(k, _)| *k == "implementation"));
        assert!(dims.iter().any(|(k, _)| *k == "test_strategy"));
        assert!(!dims.iter().any(|(k, _)| *k == "architecture"));
        assert!(!dims.iter().any(|(k, _)| *k == "data_model"));
    }

    #[test]
    fn test_analysis_dimensions_impl_with_devops() {
        let dims = analysis_dimensions(&["devops".into(), "deployment".into()], PhaseGroup::Implementation);
        assert!(dims.iter().any(|(k, _)| *k == "deployment"));
        assert!(dims.iter().any(|(k, _)| *k == "monitoring"));
    }

    #[test]
    fn test_analysis_dimensions_impl_with_storage() {
        let dims = analysis_dimensions(&["database".into(), "storage".into()], PhaseGroup::Implementation);
        assert!(dims.iter().any(|(k, _)| *k == "data_integrity"));
        assert!(dims.iter().any(|(k, _)| *k == "query_performance"));
    }

    #[test]
    fn test_analysis_dimensions_report_group() {
        let dims = analysis_dimensions(&[], PhaseGroup::Report);
        assert!(dims.iter().any(|(k, _)| *k == "report"));
        assert!(dims.iter().any(|(k, _)| *k == "risk_assessment"));
    }

    #[test]
    fn test_analysis_dimensions_dedup() {
        let dims = analysis_dimensions(
            &["backend".into(), "backend".into(), "api".into()],
            PhaseGroup::Design,
        );
        let api_design_count = dims.iter().filter(|(k, _)| *k == "api_design").count();
        assert_eq!(api_design_count, 1, "dimensions must be deduplicated");
    }

    #[test]
    fn test_parse_task_tree_with_sketched() {
        let json = r#"{
            "goal": {"specific": "test"},
            "tasks": [
                {"role": "Architect", "title": "Design API", "objective": "Design the API", "sketched": false},
                {"role": "Developer", "title": "Implement API", "objective": "Implement the API", "sketched": true}
            ],
            "plan_steps": [
                {"phase": "design", "description": "Design phase", "tasks": ["Design API"], "phase_group": "design"},
                {"phase": "development", "description": "Impl phase", "tasks": ["Implement API"], "phase_group": "implementation"}
            ]
        }"#;
        let tree = parse_task_tree(json).expect("should parse");
        assert_eq!(tree.tasks.len(), 2);
        assert!(!tree.tasks[0].sketched);
        assert!(tree.tasks[1].sketched);
        assert_eq!(tree.plan_steps[0].phase_group, "design");
        assert_eq!(tree.plan_steps[1].phase_group, "implementation");
    }

    #[test]
    fn test_parse_task_tree_defaults_sketched_false() {
        let json = r#"{"goal": {"specific": "test"}, "tasks": [{"role": "Dev", "title": "T1", "objective": "Do it"}], "plan_steps": []}"#;
        let tree = parse_task_tree(json).expect("should parse");
        assert!(!tree.tasks[0].sketched);
        assert_eq!(tree.tasks[0].estimated_cycles, 5);
        assert_eq!(tree.tasks[0].priority, "medium");
    }

    #[test]
    fn test_folder_summary_deserialization() {
        let json = r#"{"one_line_summary":"All 4 AC passed","key_findings":["AC met","Output matches"]}"#;
        let s: FolderSummary = serde_json::from_str(json).expect("should parse");
        assert_eq!(s.one_line_summary, "All 4 AC passed");
        assert_eq!(s.key_findings.len(), 2);
    }

    #[test]
    fn test_folder_summary_defaults() {
        let s: FolderSummary = serde_json::from_str("{}").expect("should parse");
        assert!(s.one_line_summary.is_empty());
        assert!(s.key_findings.is_empty());
    }

    #[test]
    fn test_fallback_one_liner_skips_markdown() {
        let text = "## 验收标准评估\n| 标准 | 状态 |\nFINAL: PASS — All criteria met";
        let result = fallback_one_liner(text, 4, 4);
        assert!(!result.starts_with("##"));
        assert!(!result.starts_with("|"));
    }

    #[test]
    fn test_fallback_one_liner_finds_meaningful() {
        let text = "## Header\n\n\n  First meaningful line after blanks";
        let result = fallback_one_liner(text, 3, 5);
        assert_eq!(result, "First meaningful line after blanks");
    }
}
