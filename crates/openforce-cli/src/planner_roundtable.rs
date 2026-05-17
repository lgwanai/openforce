use openforce_llm_client::LlmClient;
use openforce_knowledge_base::ClassificationResult;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct TaskTree {
    pub goal: SmartGoal, pub data_sources: Vec<String>,
    pub tasks: Vec<DecomposedTask>, pub mece_validated: bool,
    pub confidence: String, pub needs_info: Vec<String>,
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

/// Shared thinking framework injected into every agent prompt.
const THINKING_FRAMEWORK: &str = "\
你必须严格遵循以下三步思考法，不得跳过任何一步:\n\n\
## 步骤1: 了解现状 — 信息缺口检测\n\
- 列出你已经知道的信息(从任务描述和项目结构中)\n\
- 列出你缺少的关键信息(文件内容? 外部知识? 用户意图?)\n\
- 对每个缺口判断: [需询问用户: 具体问题] 或 [需网络检索: 搜索关键词]\n\
- 如果信息充分，标注 [信息充足，可继续]\n\n\
## 步骤2: 分解探讨 — 坚定立场，充分展开\n\
- 基于现有信息，提出你的拆解方案\n\
- 从你的维度(交互/数据/实施)深入分析每个子任务\n\
- 自检: 这个拆解是否 MECE (相互独立、完全穷尽)?\n\
- 标注 [MECE验证通过] 或 [MECE问题: 具体描述]\n\n\
## 步骤3: 输出最优解\n\
- 综合考虑所有信息，输出当前最优的子任务列表\n\
- 每个子任务必须: 有角色分配、有文件路径(如适用)、有验收标准\n\
- 如果步骤1发现信息缺口且无法立即解决，标注 [待补充: 内容]\n\
- 格式: [角色名] 任务标题: 任务描述 | 验收: 标准";

/// RoundTable: 3 agents × 3 rounds → MECE-validated task tree.
pub async fn run_roundtable(
    planner: &LlmClient, task: &str,
    classification: &ClassificationResult,
    available_roles: &[String], dir_summary: &str,
) -> Result<TaskTree, String> {
    let roles_str = available_roles.join(", ");
    let mut needs_info: Vec<String> = vec![];

    // ── Round 1: 3 independent proposals with THINKING_FRAMEWORK ──
    let (i_text, d_text, imp_text) = {
        let i_prompt = format!(
            "{framework}\n\n---\n你现在的角色: 交互/用户体验架构师\n分析维度: 用户交互流程、体验一致性、端到端可用性\n\n任务: {task}\n可用角色: [{roles}]\n项目结构:\n{dir}\n\n开始你的三步思考:",
            framework = THINKING_FRAMEWORK, roles = roles_str, dir = dir_summary
        );
        let d_prompt = format!(
            "{framework}\n\n---\n你现在的角色: 数据/系统架构师\n分析维度: 数据流、API、文件系统、数据库、状态管理\n\n任务: {task}\n可用角色: [{roles}]\n项目结构:\n{dir}\n\n开始你的三步思考:",
            framework = THINKING_FRAMEWORK, roles = roles_str, dir = dir_summary
        );
        let imp_prompt = format!(
            "{framework}\n\n---\n你现在的角色: 实施/工程架构师\n分析维度: 执行可行性、时间顺序、依赖关系、技术约束\n\n任务: {task}\n可用角色: [{roles}]\n项目结构:\n{dir}\n\n开始你的三步思考:",
            framework = THINKING_FRAMEWORK, roles = roles_str, dir = dir_summary
        );

        let (i, d, imp) = tokio::join!(
            planner.chat("你是交互架构师。遵循三步思考法。", &i_prompt),
            planner.chat("你是数据架构师。遵循三步思考法。", &d_prompt),
            planner.chat("你是实施架构师。遵循三步思考法。", &imp_prompt),
        );
        (i.map(|(t,_)| t).unwrap_or_default(), d.map(|(t,_)| t).unwrap_or_default(), imp.map(|(t,_)| t).unwrap_or_default())
    };
    if i_text.is_empty() || d_text.is_empty() || imp_text.is_empty() {
        return Err("RoundTable: agent proposal failed".into());
    }

    // Collect information needs from Round 1
    for text in [&i_text, &d_text, &imp_text] {
        for line in text.lines() {
            if line.contains("[需询问用户:") || line.contains("[需网络检索:") {
                needs_info.push(line.trim().to_string());
            }
        }
    }

    // Execute REAL web searches for flagged queries (via deerflow-skill Tavily API)
    let mut search_results = String::new();
    for info in &needs_info {
        if info.contains("[需网络检索:") {
            if let Some(q) = info.split("[需网络检索:").nth(1).and_then(|s| s.split(']').next()) {
                let result = crate::web_search::web_search(q).await;
                search_results.push_str(&format!("\n🔍 '{q}':\n{result}\n"));
            }
        }
    }

    // ── Round 2: Cross-Review (with THINKING_FRAMEWORK) ──
    let review_prompt = format!(
        "{framework}\n\n---\n你现在的角色: 任务拆解审查员\n\n=== 交互维度方案 ===\n{i}\n\n=== 数据维度方案 ===\n{d}\n\n=== 实施维度方案 ===\n{imp}\n\n=== 信息缺口汇总 ===\n{gaps}\n\n=== 检索结果 ===\n{search}\n\n开始你的三步思考:\n步骤1: 三个方案中哪些信息缺口需要优先解决?\n步骤2: 交叉审查 — MECE 重叠? 遗漏? 致命缺陷?\n步骤3: 输出合并后的最优子任务列表(格式: [角色] 任务名: 描述: 验收标准)",
        framework = THINKING_FRAMEWORK, i = i_text, d = d_text, imp = imp_text,
        gaps = needs_info.join("\n"), search = search_results
    );
    let (review_text, _) = planner.chat("你是资深项目审查员。遵循三步思考法。不确定标注[需确认]。", &review_prompt)
        .await.map_err(|e| format!("cross-review: {e}"))?;

    // ── Round 3: Final Synthesis ──
    let synth_prompt = format!(
        "{framework}\n\n---\n你现在的角色: 最终决策者\n\n原始任务: {task}\n审查结果:\n{review}\n\n步骤3 — 输出最优解。输出严格JSON:\n{{\"goal\":{{\"specific\":\"...\",\"measurable\":\"...\",\"achievable\":\"...\",\"relevant\":\"...\",\"time_bound\":\"...\"}},\"data_sources\":[\"路径\"],\"tasks\":[{{\"role\":\"角色\",\"title\":\"标题\",\"objective\":\"目标\",\"files\":[\"路径\"],\"steps\":[\"步骤\"],\"acceptance_criteria\":[\"标准\"],\"dependencies\":[]}}],\"mece_validated\":true,\"confidence\":\"high|medium|low\"}}\n\nrole从[{roles}]选。files必须真实存在。不确定设confidence=low。仅JSON。",
        framework = THINKING_FRAMEWORK, review = review_text, roles = roles_str
    );
    let (final_json, _) = planner.chat("你是任务拆解决策者。仅输出JSON。", &synth_prompt)
        .await.map_err(|e| format!("synthesize: {e}"))?;

    let mut tree = parse_task_tree(&final_json)?;
    tree.needs_info = needs_info;
    Ok(tree)
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
        needs_info: vec![],
    })
}
