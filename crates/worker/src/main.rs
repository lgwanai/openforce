use anyhow::Result;
use openforce_llm_client::LlmClient;
use openforce_llm_client::tool::{Tool, ToolCall, ToolResult};
use openforce_skill::{SkillConfig, SkillExecutor, SkillRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::time::{Duration, Instant};

// ── Agent Memory ──

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubTask { id: usize, description: String, status: String, output: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Decision { cycle: usize, action: String, detail: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentMemory {
    role: String, goal: String, acceptance_criteria: Vec<String>,
    subtasks: Vec<SubTask>, recent_decisions: Vec<Decision>,
    key_findings: Vec<String>, started_at: String, files_available: Vec<String>,
}

impl AgentMemory {
    fn build_context(&self) -> String {
        let mut ctx = String::new();
        ctx.push_str(&format!("ROLE: {}\nGOAL: {}\n\n", self.role, self.goal));
        ctx.push_str("ACCEPTANCE CRITERIA:\n");
        for (i, c) in self.acceptance_criteria.iter().enumerate() {
            ctx.push_str(&format!("  {}. {}\n", i + 1, c));
        }
        ctx.push_str("\n── SUBTASKS ──\n");
        for t in &self.subtasks {
            let icon = match t.status.as_str() {
                "done" => "✓", "in_progress" => "▶", "blocked" => "✗", _ => "○"
            };
            ctx.push_str(&format!("  [{icon}] {}. {}\n", t.id, t.description));
            if !t.output.is_empty() { ctx.push_str(&format!("       → {}\n", t.output)); }
        }
        let done = self.subtasks.iter().filter(|t| t.status == "done").count();
        ctx.push_str(&format!("\nProgress: {}/{}\n", done, self.subtasks.len()));
        ctx.push_str("\n── KEY FINDINGS ──\n");
        for f in &self.key_findings { ctx.push_str(&format!("  • {}\n", f)); }
        if self.key_findings.is_empty() { ctx.push_str("  (none yet)\n"); }
        ctx.push_str("\n── RECENT DECISIONS ──\n");
        for d in self.recent_decisions.iter().rev().take(5) {
            ctx.push_str(&format!("  [C{}] {}: {}\n", d.cycle, d.action, d.detail));
        }
        if !self.files_available.is_empty() {
            ctx.push_str(&format!("\n── AVAILABLE FILES ({} total) ──\n", self.files_available.len()));
            for f in self.files_available.iter().take(30) { ctx.push_str(&format!("  {}\n", f)); }
        }
        ctx
    }

    fn add_decision(&mut self, cycle: usize, action: &str, detail: &str) {
        self.recent_decisions.push(Decision { cycle, action: action.into(), detail: detail.into() });
        if self.recent_decisions.len() > 20 { self.recent_decisions.remove(0); }
    }

    fn add_finding(&mut self, finding: &str) {
        let s: String = finding.to_string();
        if !self.key_findings.contains(&s) { self.key_findings.push(s); }
    }

    fn all_done(&self) -> bool { self.subtasks.iter().all(|t| t.status == "done") }
}

// ── Worker Task Input ──

#[derive(Debug, Deserialize)]
struct WorkerTask {
    task: String, subtask: String, profile_name: String,
    #[serde(default)] provider: String,
    model: String, system_prompt: String,
    api_key: Option<String>, base_url: Option<String>,
    output_file: Option<String>, review_paths: Vec<String>,
    #[serde(default)] skill_metadata: Option<String>,
    #[serde(default)] skills_dir: Option<String>,
    #[serde(default)] bound_skill: Option<String>,
}

// ── Tool Definitions ──

fn worker_tools() -> Vec<Tool> {
    vec![
        Tool::simple("read_file", "Read a file from available files", "path", "File path to read"),
        Tool::new("write_file", "Write content to a file", serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to write"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path", "content"]
        })),
        Tool::new("shell_exec", "Execute a shell command (30s timeout)", serde_json::json!({
            "type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]
        })),
        Tool::bare("mark_all_done", "Mark all subtasks as completed"),
        Tool::new("record_finding", "Record a key finding", serde_json::json!({
            "type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]
        })),
        Tool::new("skill_load", "Load a skill's instructions", serde_json::json!({
            "type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]
        })),
        Tool::new("skill_load_ref", "Load a reference file from a skill dir", serde_json::json!({
            "type": "object",
            "properties": {"skill": {"type": "string"}, "path": {"type": "string"}},
            "required": ["skill", "path"]
        })),
        Tool::new("skill_exec_script", "Execute a script from a skill dir", serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {"type": "string"}, "script": {"type": "string"},
                "args": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["skill", "script"]
        })),
    ]
}

// ── Tool Execution ──

struct ToolExecutor {
    memory: AgentMemory,
    active_file_content: Option<(String, String)>,
    skill_registry: SkillRegistry,
}

fn parse_tool_args(call: &ToolCall, required: &[&str]) -> Result<std::collections::HashMap<String, String>, String> {
    let args: Value = serde_json::from_str(&call.arguments).map_err(|e| format!("parse args: {e}"))?;
    let mut map = std::collections::HashMap::new();
    for key in required {
        let val = args.get(*key).and_then(|v| v.as_str()).ok_or_else(|| format!("missing arg: {key}"))?;
        map.insert(key.to_string(), val.to_string());
    }
    Ok(map)
}

impl ToolExecutor {
    async fn execute(&mut self, call: &ToolCall) -> ToolResult {
        match call.name.as_str() {
            "read_file" => self.execute_read(call).await,
            "write_file" => self.execute_write(call),
            "shell_exec" => self.execute_shell(call).await,
            "record_finding" => self.execute_finding(call),
            "mark_all_done" => self.execute_all_done(call),
            "skill_load" => self.execute_skill_load(call),
            "skill_load_ref" => self.execute_skill_ref(call),
            "skill_exec_script" => self.execute_skill_script(call).await,
            _ => ToolResult::error(&call.id, &format!("Unknown tool: {}", call.name)),
        }
    }

    async fn execute_read(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["path"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let path = &args["path"];
        let matched = self.memory.files_available.iter()
            .find(|f| f.ends_with(path))
            .or_else(|| {
                let fname = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                self.memory.files_available.iter().find(|f| f.ends_with(fname))
            }).cloned().unwrap_or_else(|| path.clone());
        match fs::read_to_string(&matched) {
            Ok(content) => {
                let truncated = if content.len() > 16000 {
                    format!("{}\n... (truncated, {} total)", safe_slice(&content, 16000), content.len())
                } else { content.clone() };
                self.active_file_content = Some((matched.clone(), truncated.clone()));
                self.memory.add_decision(0, "READ", &format!("{} ({} chars)", matched, content.len()));
                ToolResult::success(&call.id, &truncated)
            }
            Err(e) => ToolResult::error(&call.id, &format!("read {matched}: {e}")),
        }
    }

    fn execute_write(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["path", "content"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let (path, content) = (&args["path"], &args["content"]);
        if let Some(parent) = std::path::Path::new(path).parent() { let _ = fs::create_dir_all(parent); }
        match fs::write(path, content) {
            Ok(_) => {
                self.memory.add_decision(0, "WRITE", &format!("{} ({} chars)", path, content.len()));
                ToolResult::success(&call.id, &format!("Wrote {} bytes to {path}", content.len()))
            }
            Err(e) => ToolResult::error(&call.id, &format!("write {path}: {e}")),
        }
    }

    async fn execute_shell(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["command"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let cmd = &args["command"];
        let lower = cmd.to_lowercase();
        let dangerous = ["rm -rf /", "mkfs", "dd if=", "> /dev/sda", "chmod 777 /"];
        if dangerous.iter().any(|d| lower.contains(d)) {
            return ToolResult::error(&call.id, &format!("blocked dangerous: {cmd}"));
        }
        let output = tokio::time::timeout(Duration::from_secs(30),
            tokio::process::Command::new("sh").arg("-c").arg(cmd)
                .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).output()
        ).await;
        match output {
            Ok(Ok(out)) => {
                let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                let truncated = if combined.len() > 4000 { format!("{}\n... (truncated)", safe_slice(&combined, 4000)) } else { combined.clone() };
                self.active_file_content = Some((format!("SHELL:{cmd}"), truncated.clone()));
                self.memory.add_decision(0, "SHELL", &format!("{cmd} ({} chars)", combined.len()));
                ToolResult::success(&call.id, &truncated)
            }
            Ok(Err(e)) => ToolResult::error(&call.id, &format!("{cmd}: {e}")),
            Err(_) => ToolResult::error(&call.id, &format!("{cmd}: timeout (30s)")),
        }
    }

    fn execute_finding(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["text"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let text = &args["text"];
        self.memory.add_finding(text);
        self.memory.add_decision(0, "FINDING", text);
        ToolResult::success(&call.id, &format!("Finding recorded: {text}"))
    }

    fn execute_all_done(&mut self, call: &ToolCall) -> ToolResult {
        for t in &mut self.memory.subtasks {
            if t.status != "done" { t.status = "done".into(); if t.output.is_empty() { t.output = "Completed".into(); } }
        }
        ToolResult::success(&call.id, "All subtasks marked as done")
    }

    fn execute_skill_load(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["name"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let name = &args["name"];
        if let Some(body) = self.skill_registry.load_skill_body(name) {
            let dirs_info = if let Some(skill) = self.skill_registry.get(name) {
                let dirs = skill.available_support_dirs();
                if dirs.is_empty() { String::new() } else { format!("\nAvailable dirs: {}", dirs.join(", ")) }
            } else { String::new() };
            self.active_file_content = Some((format!("SKILL:{name}"), format!("{body}{dirs_info}")));
            ToolResult::success(&call.id, &format!("Skill {name} loaded ({} chars)", body.len()))
        } else {
            ToolResult::error(&call.id, &format!("Skill {name} not found"))
        }
    }

    fn execute_skill_ref(&mut self, call: &ToolCall) -> ToolResult {
        let args = match parse_tool_args(call, &["skill", "path"]) { Ok(a) => a, Err(e) => return ToolResult::error(&call.id, &e) };
        let (sn, rp) = (&args["skill"], &args["path"]);
        let executor = SkillExecutor::new(&self.skill_registry);
        match executor.load_reference(sn, rp) {
            Some(content) => {
                self.active_file_content = Some((format!("REF:{sn}/{rp}"), content.clone()));
                ToolResult::success(&call.id, &format!("Reference {sn}/{rp} loaded ({} chars)", content.len()))
            }
            None => ToolResult::error(&call.id, &format!("Reference {sn}/{rp} not found")),
        }
    }

    async fn execute_skill_script(&mut self, call: &ToolCall) -> ToolResult {
        let args: Value = match serde_json::from_str(&call.arguments) {
            Ok(v) => v, Err(e) => return ToolResult::error(&call.id, &format!("parse args: {e}")),
        };
        let (sn, sp) = match (args.get("skill").and_then(|v| v.as_str()), args.get("script").and_then(|v| v.as_str())) {
            (Some(sn), Some(sp)) => (sn, sp),
            _ => return ToolResult::error(&call.id, "missing 'skill' or 'script' arg"),
        };
        let sargs: Vec<&str> = args.get("args").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect()).unwrap_or_default();
        let executor = SkillExecutor::new(&self.skill_registry);
        match executor.execute_script(sn, sp, &sargs).await {
            Ok(output) => {
                let truncated = if output.len() > 4000 { format!("{}\n... (truncated)", safe_slice(&output, 4000)) } else { output.clone() };
                self.active_file_content = Some((format!("SCRIPT:{sn}/{sp}"), truncated.clone()));
                ToolResult::success(&call.id, &truncated)
            }
            Err(e) => ToolResult::error(&call.id, &format!("script {sn}/{sp}: {e}")),
        }
    }
}

// ── Context Management ──

struct ContextManager { model: String }

const MODEL_LIMITS: &[(&str, usize)] = &[
    ("claude-opus-4", 200_000), ("claude-sonnet-4", 200_000), ("claude-haiku", 200_000),
    ("gpt-4", 128_000), ("gpt-4o", 128_000), ("gemini-2", 1_000_000), ("gemini-1.5", 1_000_000),
    ("deepseek", 128_000), ("qwen", 128_000),
];

impl ContextManager {
    fn new(model: &str) -> Self { Self { model: model.to_lowercase() } }
    fn context_limit(&self) -> usize {
        for (prefix, limit) in MODEL_LIMITS { if self.model.contains(prefix) { return *limit; } }
        128_000
    }
    fn reserved_tokens(&self) -> usize { 20_000 }
    fn usable_tokens(&self) -> usize { (self.context_limit() - self.reserved_tokens()).max(4_000) }
    fn is_overflow(&self, total_tokens: usize) -> bool { total_tokens >= self.usable_tokens() }
    fn estimate_tokens(text: &str) -> usize { text.len() / 4 }
}

// ── Compaction ──

async fn perform_compaction(client: &LlmClient, system: &str, history: &str) -> Result<String, String> {
    let cs = format!("{system}\n\nSummarize the conversation history. Output structured summary:\n\
        ## Goal\n- [summary]\n\n\
        ## Progress (Done/In Progress/Blocked)\n\n\
        ## Key Decisions\n\n\
        ## Next Steps\n\n\
        ## Critical Context\n\n\
        ## Relevant Files");
    client.chat(&cs, &format!("Summarize:\n\n{history}")).await
        .map(|(t, _)| t).map_err(|e| format!("compaction: {e}"))
}

// ── Helpers ──

fn extract_json(s: &str) -> Option<&str> { let start = s.find('{')?; let end = s.rfind('}')?; Some(&s[start..=end]) }

fn safe_slice(s: &str, max_len: usize) -> &str {
    let end = s.char_indices().nth(max_len).map(|(i, _)| i).unwrap_or(s.len());
    &s[..end]
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { s.to_string() } else {
        let end = s.char_indices().nth(max_len).map(|(i, _)| i).unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

fn build_worker_output(executor: &ToolExecutor, task: &WorkerTask, cycles: usize, tokens: usize, text: &str, passed: bool) -> Value {
    let done = executor.memory.subtasks.iter().filter(|t| t.status == "done").count();
    serde_json::json!({
        "success": passed, "action": if passed { "completed" } else { "failed_criteria" },
        "profile": task.profile_name, "model": task.model, "cycles": cycles, "tokens": tokens,
        "output": text, "subtasks_completed": done, "subtasks_total": executor.memory.subtasks.len(),
        "acceptance_criteria": executor.memory.acceptance_criteria,
        "memory_snapshot": {
            "role": executor.memory.role, "goal": executor.memory.goal,
            "subtasks": executor.memory.subtasks,
            "key_findings": executor.memory.key_findings,
            "recent_decisions": executor.memory.recent_decisions
        }
    })
}

fn write_output(task: &WorkerTask, output: &Value) {
    let out_path = task.output_file.clone()
        .unwrap_or_else(|| format!("/tmp/worker_output_{}.json", uuid::Uuid::now_v7()));
    let _ = fs::write(&out_path, serde_json::to_string_pretty(output).unwrap_or_default());
    println!("DONE: {out_path}");
}

fn build_context_string(executor: &ToolExecutor) -> String {
    let mut ctx = executor.memory.build_context();
    if let Some((ref path, ref content)) = executor.active_file_content {
        ctx.push_str(&format!("\n── READING: {path} ──\n{content}\n"));
    }
    ctx
}

// ── Main ──

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let task_file = if args.len() >= 3 && args[1] == "--task-file" { args[2].clone() }
    else { eprintln!("Usage: worker --task-file <path>"); std::process::exit(1); };

    let task_json = fs::read_to_string(&task_file)?;
    let task: WorkerTask = serde_json::from_str(&task_json)?;

    let api_key = task.api_key.as_deref().unwrap_or(
        &std::env::var("API_KEY").unwrap_or_default()).to_string();
    let base_url = task.base_url.as_deref().unwrap_or(
        &std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into())).to_string();
    if api_key.len() < 5 { eprintln!("FATAL: no API key"); std::process::exit(1); }

    let provider = if task.provider.is_empty() { "openai" } else { &task.provider };
    let client = match provider {
        "anthropic" => LlmClient::anthropic(api_key, Some(base_url)).with_model(&task.model),
        _ => LlmClient::openai(api_key, base_url, task.model.clone())
    };

    let start = Instant::now();
    let max_dur = Duration::from_secs(
        std::env::var("WORKER_MAX_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(1200));
    let max_cycles: usize = std::env::var("WORKER_MAX_CYCLES").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
    let max_no_prog: usize = std::env::var("WORKER_MAX_NO_PROGRESS").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let mut tokens_used: usize = 0;
    let max_tokens: usize = std::env::var("WORKER_MAX_TOKENS").ok().and_then(|s| s.parse().ok()).unwrap_or(2_000_000);

    let system = if task.system_prompt.is_empty() {
        format!("You are a {}. Use the provided tools to complete subtasks. After each subtask, self-verify against its criterion and respond VERIFIED or FAILED.", task.profile_name)
    } else { task.system_prompt.clone() };

    // Discover skills
    let skills_dir = task.skills_dir.as_deref().unwrap_or("skills");
    let skill_config = SkillConfig::load_from_skills_dir(skills_dir);
    let skill_registry = SkillRegistry::discover_with_config(skills_dir, Some(&skill_config));
    if !skill_registry.is_empty() {
        eprintln!("[Skills] {} available", skill_registry.enabled_skill_names().len());
    }

    // Auto-load bound skill body into initial context (progressive disclosure Level 2)
    let bound_skill_block = if let Some(ref bs_name) = task.bound_skill {
        if let Some(body) = skill_registry.load_skill_body(bs_name) {
            eprintln!("[BoundSkill] /{} auto-loaded ({} chars)", bs_name, body.len());
            format!("\n\n<bound_skill name=\"{bs_name}\">\n{body}\n</bound_skill>\n")
        } else { String::new() }
    } else { String::new() };

    // Phase 0: Decompose
    let dp = format!(
        "{bs}\nRole: {r}\nTask: {t}\nSubtask: {s}\n\nAvailable files ({n}):\n{files}\n\n\
         Define 2-4 acceptance criteria and 2-4 subtasks. JSON ONLY:\n\
         {{\"acceptance_criteria\":[\"...\"],\"subtasks\":[{{\"id\":1,\"description\":\"...\"}}]}}",
        bs = bound_skill_block, r=task.profile_name, t=task.task, s=task.subtask, n=task.review_paths.len(),
        files=task.review_paths.iter().take(40).map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n"));

    let (plan_text, pt) = client.chat(&system, &dp).await?;
    tokens_used += pt as usize;

    let plan: Value = serde_json::from_str(&plan_text).unwrap_or_else(|_| {
        serde_json::from_str(extract_json(&plan_text).unwrap_or("{}")).unwrap_or_else(|_| {
            serde_json::json!({"acceptance_criteria":["Complete"],"subtasks":[{"id":1,"description":task.subtask.clone()}]})
        })
    });

    let ac: Vec<String> = plan["acceptance_criteria"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["Complete the task".into()]);

    let subs: Vec<SubTask> = plan["subtasks"].as_array()
        .map(|a| a.iter().enumerate().map(|(i,v)| SubTask {
            id: v["id"].as_u64().unwrap_or(i as u64 + 1) as usize,
            description: v["description"].as_str().unwrap_or("review").into(),
            status: "pending".into(), output: String::new(),
        }).collect())
        .unwrap_or_else(|| vec![SubTask{id:1,description:task.subtask.clone(),status:"pending".into(),output:String::new()}]);

    let memory = AgentMemory {
        role: task.profile_name.clone(), goal: format!("{}: {}", task.task, task.subtask),
        acceptance_criteria: ac, subtasks: subs, recent_decisions: vec![], key_findings: vec![],
        started_at: chrono::Utc::now().to_rfc3339(), files_available: task.review_paths.clone(),
    };

    let tools = worker_tools();
    let ctx_mgr = ContextManager::new(&task.model);
    let mut executor = ToolExecutor { memory, active_file_content: None, skill_registry };
    let mut conversation: Vec<openforce_llm_client::unified::ToolMessage> = Vec::new();

    eprintln!("[Agent] {} subtasks, {} criteria, {} files",
        executor.memory.subtasks.len(), executor.memory.acceptance_criteria.len(),
        executor.memory.files_available.len());

    // ── Phase 1: Criteria-Driven Execution ──
    // Each subtask has a clear acceptance criterion. Fulfill one at a time, verify, and move on.
    // No unbounded loop — criteria met = done.
    let mut cycles: usize = 0;
    let criteria_list: Vec<String> = executor.memory.acceptance_criteria.clone();
    let subtask_count = executor.memory.subtasks.len();

    'subtask_loop: for st_idx in 0..subtask_count {
        executor.memory.subtasks[st_idx].status = "in_progress".into();
        let st_desc = executor.memory.subtasks[st_idx].description.clone();
        let criterion = criteria_list.get(st_idx).cloned().unwrap_or_else(|| "Complete".into());
        eprintln!("[Subtask {}/{}] {}", st_idx+1, subtask_count, truncate_str(&st_desc, 100));

        let max_st = (max_cycles / subtask_count.max(1)).clamp(5, 20);

        for st_cycle in 0..max_st {
            cycles += 1;
            if start.elapsed() > max_dur {
                let d = executor.memory.subtasks.iter().filter(|t| t.status == "done").count();
                write_output(&task, &serde_json::json!({"success":d>0,"action":if d>0{"completed_partial"}else{"timeout"},"subtasks_done":d,"subtasks_total":subtask_count}));
                return Ok(());
            }
            if tokens_used >= max_tokens {
                let d = executor.memory.subtasks.iter().filter(|t| t.status == "done").count();
                write_output(&task, &serde_json::json!({"success":d>0,"action":if d>0{"completed_partial"}else{"token_budget"},"subtasks_done":d,"subtasks_total":subtask_count}));
                return Ok(());
            }

            // Compact if conversation grows too large
            if ctx_mgr.is_overflow(conversation.iter().map(|m| ContextManager::estimate_tokens(&m.content) + 100).sum()) {
                conversation.clear();
                conversation.push(openforce_llm_client::unified::ToolMessage::user(&format!("[COMPACTED: {} subtasks done]", executor.memory.subtasks.iter().filter(|t| t.status == "done").count())));
            }

            let ctx = build_context_string(&executor);
            let um = if st_cycle == 0 {
                format!("{skill}Goal: {role} — {goal}\nSUBTASK #{n}/{t}: {st}\nCRITERION: {crit}\n\nFiles ({fc}):\n{files}\n\nComplete this subtask. When done, verify against the criterion above.\nIf criterion is met → respond: VERIFIED\nIf not met → respond: FAILED: <reason>, then retry.",
                    skill=task.skill_metadata.as_deref().unwrap_or(""),
                    role=executor.memory.role, goal=executor.memory.goal, n=st_idx+1, t=subtask_count, st=st_desc, crit=criterion,
                    fc=executor.memory.files_available.len(),
                    files=executor.memory.files_available.iter().take(20).map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n"))
            } else {
                format!("SUBTASK #{n}: {st}\nCriterion: {crit}\nRetry {rc}/{m}\nUse tools, fulfill the criterion, respond VERIFIED or FAILED.",
                    n=st_idx+1, st=st_desc, crit=criterion, rc=st_cycle+1, m=max_st)
            };

            conversation.push(openforce_llm_client::unified::ToolMessage::user(&um));

            match client.chat_with_tools(&system, &conversation, &tools).await {
                Ok(response) => {
                    tokens_used += response.tokens_used as usize;
                    if response.has_tool_calls() {
                        conversation.push(openforce_llm_client::unified::ToolMessage::assistant_with_tools(&response.text, &response.tool_calls));
                        let mut tr = Vec::new();
                        for tc in &response.tool_calls {
                            let r = executor.execute(tc).await;
                            eprintln!("    {} {}", if r.success {"✓"} else {"✗"}, tc.name);
                            tr.push(r);
                        }
                        conversation.push(openforce_llm_client::unified::ToolMessage::tool_results(&tr));
                        continue;
                    }
                    let text = &response.text;
                    conversation.push(openforce_llm_client::unified::ToolMessage {
                        role: "assistant".into(), content: text.clone(),
                        tool_calls: None, tool_results: None, tool_call_id: None,
                    });
                    let upper = text.to_uppercase();
                    if upper.contains("VERIFIED") || upper.contains("FINAL: PASS") {
                        executor.memory.subtasks[st_idx].status = "done".into();
                        executor.memory.subtasks[st_idx].output = text.clone();
                        eprintln!("  ✓ Subtask {}/{} PASS", st_idx+1, subtask_count);
                        continue 'subtask_loop;
                    } else if upper.contains("FAILED") {
                        eprintln!("  ✗ Subtask {}/{} FAIL, retry {}/{}", st_idx+1, subtask_count, st_cycle+1, max_st);
                    } else {
                        executor.memory.add_decision(cycles, "WORKING", &truncate_str(text, 120));
                    }
                }
                Err(e) => {
                    eprintln!("[LLM error subtask {}]: {e}", st_idx+1);
                    write_output(&task, &serde_json::json!({"success":false,"action":"error","detail":format!("{e}")}));
                    std::process::exit(1);
                }
            }
        }
        executor.memory.subtasks[st_idx].status = "failed".into();
        executor.memory.subtasks[st_idx].output = format!("Retries exhausted ({})", max_st);
    }
    // Phase 2: Verification
    let vp = format!("{}\n\nFINAL: rate each criterion PASS/FAIL. End with: FINAL: PASS|FAIL", build_context_string(&executor));
    match client.chat(&system, &vp).await {
        Ok((text, _)) => {
            let passed = text.to_uppercase().contains("FINAL: PASS");
            write_output(&task, &build_worker_output(&executor, &task, cycles, tokens_used, &text, passed));
        }
        Err(e) => {
            write_output(&task, &serde_json::json!({"success":false,"action":"error","reason":"verification_failed","detail":format!("{e}")}));
        }
    }
    Ok(())
}
