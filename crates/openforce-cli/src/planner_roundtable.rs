use openforce_llm_client::LlmClient;
use openforce_knowledge_base::ClassificationResult;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct TaskTree {
    pub goal: SmartGoal,
    pub data_sources: Vec<String>,
    pub tasks: Vec<DecomposedTask>,
    pub mece_validated: bool,
    pub confidence: String,
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
}

/// RoundTable: 3 agents (交互/数据/实施) × 3 rounds (提案/交叉审查/合成) → MECE 验证的任务树
pub async fn run_roundtable(
    planner: &LlmClient, task: &str,
    classification: &ClassificationResult,
    available_roles: &[String], dir_summary: &str,
) -> Result<TaskTree, String> {
    let roles_str = available_roles.join(", ");

    // ── Round 1: 3-way independent proposals ──
    let (i_text, d_text, imp_text) = {
        let i_prompt = format!("你是交互/用户体验架构师。从用户交互维度分析任务拆解。\n目标: {task}\n可用角色: [{roles_str}]\n项目结构:\n{dir_summary}\n\n请输出:\n1. 交互维度的子任务列表 (每个标注 [角色] 任务描述)\n2. 验收标准\n3. 自检: 你的拆解是否 MECE? 如有遗漏请说明。不确定时标注 [需确认]");
        let d_prompt = format!("你是数据/系统架构师。从数据流和系统集成维度分析任务拆解。\n目标: {task}\n可用角色: [{roles_str}]\n项目结构:\n{dir_summary}\n\n请输出:\n1. 所需数据源和文件路径\n2. 数据维度的子任务列表 (每个标注 [角色] 任务描述)\n3. 处理步骤\n4. 自检: MECE? 与交互维度是否重叠或遗漏? 不确定标注 [需确认]");
        let imp_prompt = format!("你是实施/工程架构师。从执行可行性和顺序维度分析任务拆解。\n目标: {task}\n可用角色: [{roles_str}]\n项目结构:\n{dir_summary}\n\n请输出:\n1. 按逻辑顺序的执行计划\n2. 实施维度的子任务列表 (每个标注 [角色] 任务描述)\n3. 依赖关系\n4. 自检: MECE? 有致命缺陷或遗漏? 不确定标注 [需确认]");

        let (i, d, imp) = tokio::join!(
            planner.chat("你是交互架构师，从UX和交互流程维度拆解任务。必须引用具体文件路径。不确定标注[需确认]。", &i_prompt),
            planner.chat("你是数据架构师，从数据流/API/文件系统维度拆解任务。必须引用具体文件路径。不确定标注[需确认]。", &d_prompt),
            planner.chat("你是实施架构师，从执行可行性/时间顺序/依赖维度拆解任务。不确定标注[需确认]。", &imp_prompt),
        );
        (i.map(|(t,_)|t).unwrap_or_default(), d.map(|(t,_)|t).unwrap_or_default(), imp.map(|(t,_)|t).unwrap_or_default())
    };
    if i_text.is_empty() || d_text.is_empty() || imp_text.is_empty() {
        return Err("RoundTable: agent proposal failed".into());
    }

    // ── Round 2: Cross-Review ──
    let review_prompt = format!(
        "你是任务拆解审查员。以下是三位架构师的拆解方案。请交叉审查:\n\n=== 交互维度 ===\n{i_text}\n\n=== 数据维度 ===\n{d_text}\n\n=== 实施维度 ===\n{imp_text}\n\n检查:\n1. MECE: 是否有重叠? 是否有遗漏?\n2. 致命缺陷: 是否有不可执行或逻辑矛盾的子任务?\n3. 输出合并后的子任务列表 (格式: [角色] 任务名: 描述: 验收标准)\n不确定时标注 [需确认: 具体问题]。不要猜测。"
    );
    let (review_text, _) = planner.chat("你是资深项目审查员，交叉验证任务拆解。不确定标注[需确认]。不要自作主张。", &review_prompt)
        .await.map_err(|e| format!("cross-review: {e}"))?;

    // ── Round 3: Synthesize ──
    let synth_prompt = format!(
        "你是最终决策者。基于审查结果生成JSON任务树。\n原始任务: {task}\n审查结果:\n{review_text}\n\n输出严格JSON:\n{{\"goal\":{{\"specific\":\"...\",\"measurable\":\"...\",\"achievable\":\"...\",\"relevant\":\"...\",\"time_bound\":\"...\"}},\"data_sources\":[\"路径\"],\"tasks\":[{{\"role\":\"角色\",\"title\":\"标题\",\"objective\":\"目标\",\"files\":[\"路径\"],\"steps\":[\"步骤\"],\"acceptance_criteria\":[\"标准\"],\"dependencies\":[]}}],\"mece_validated\":true,\"confidence\":\"high|medium|low\"}}\nrole从[{roles_str}]选。files必须真实存在。不确定设confidence=low。仅JSON，无其他文字。"
    );
    let (final_json, _) = planner.chat("你是任务拆解决策者。仅输出JSON。", &synth_prompt)
        .await.map_err(|e| format!("synthesize: {e}"))?;
    parse_task_tree(&final_json)
}

fn parse_task_tree(json_str: &str) -> Result<TaskTree, String> {
    let json = json_str.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    #[derive(Deserialize)] struct R { goal: G, #[serde(default)] data_sources: Vec<String>, tasks: Vec<T>, #[serde(default)] mece_validated: bool, #[serde(default)] confidence: String }
    #[derive(Deserialize)] struct G { #[serde(default)] specific: String, #[serde(default)] measurable: String, #[serde(default)] achievable: String, #[serde(default)] relevant: String, #[serde(default)] time_bound: String }
    #[derive(Deserialize)] struct T { #[serde(default)] role: String, #[serde(default)] title: String, #[serde(default)] objective: String, #[serde(default)] files: Vec<String>, #[serde(default)] steps: Vec<String>, #[serde(default)] acceptance_criteria: Vec<String>, #[serde(default)] dependencies: Vec<String> }
    let r: R = serde_json::from_str(json).map_err(|e| format!("parse: {e} | JSON: {json:.300}"))?;
    Ok(TaskTree {
        goal: SmartGoal { specific: r.goal.specific, measurable: r.goal.measurable, achievable: r.goal.achievable, relevant: r.goal.relevant, time_bound: r.goal.time_bound },
        data_sources: r.data_sources,
        tasks: r.tasks.into_iter().map(|t| DecomposedTask { role: t.role, title: t.title, objective: t.objective, files: t.files, steps: t.steps, acceptance_criteria: t.acceptance_criteria, dependencies: t.dependencies }).collect(),
        mece_validated: r.mece_validated, confidence: r.confidence,
    })
}
