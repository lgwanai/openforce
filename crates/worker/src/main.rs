use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{Duration, Instant};

// ── Task State (persistent task tracking) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubTask {
    id: usize,
    description: String,
    status: String,  // pending | in_progress | done | blocked
    output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskState {
    goal: String,
    role: String,
    acceptance_criteria: Vec<String>,
    subtasks: Vec<SubTask>,
    context: String,
    started_at: String,
}

impl TaskState {
    fn summary(&self) -> String {
        let mut s = format!("GOAL: {}\nROLE: {}\n\nACCEPTANCE CRITERIA:\n", self.goal, self.role);
        for (i, c) in self.acceptance_criteria.iter().enumerate() {
            s.push_str(&format!("  {}. {c}\n", i + 1));
        }
        s.push_str("\nTASK LIST:\n");
        for t in &self.subtasks {
            let icon = match t.status.as_str() {
                "done" => "[✓]", "in_progress" => "[▶]", "blocked" => "[✗]", _ => "[ ]"
            };
            s.push_str(&format!("  {icon} {}. {}\n", t.id, t.description));
            if !t.output.is_empty() { s.push_str(&format!("       → {}\n", t.output)); }
        }
        let done = self.subtasks.iter().filter(|t| t.status == "done").count();
        let total = self.subtasks.len();
        s.push_str(&format!("\nProgress: {done}/{total} tasks complete\n"));
        if !self.context.is_empty() { s.push_str(&format!("\nCONTEXT:\n{}\n", self.context)); }
        s
    }

    fn all_done(&self) -> bool {
        self.subtasks.iter().all(|t| t.status == "done")
    }

    fn pending_count(&self) -> usize {
        self.subtasks.iter().filter(|t| t.status == "pending").count()
    }
}

// ── Worker Task Input ──

#[derive(Debug, Deserialize)]
struct WorkerTask {
    task: String,
    subtask: String,
    profile_name: String,
    model: String,
    system_prompt: String,
    api_key: Option<String>,
    base_url: Option<String>,
    output_file: Option<String>,
    review_paths: Vec<String>,
    project_tools_addr: Option<String>,
    session_id: Option<String>,
    session_map: Option<String>,
    redis_url: Option<String>,
}

// ── Tools ──

async fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))
}

async fn list_dir(dir: &str) -> Result<Vec<String>, String> {
    let mut files = vec![];
    for entry in std::fs::read_dir(dir).map_err(|e| format!("readdir {dir}: {e}"))? {
        let e = entry.map_err(|e| format!("entry: {e}"))?;
        if e.path().is_file() { files.push(e.path().display().to_string()); }
    }
    Ok(files)
}

// ── Main ──

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let task_file = if args.len() >= 3 && args[1] == "--task-file" {
        args[2].clone()
    } else {
        eprintln!("Usage: worker --task-file <path>");
        std::process::exit(1);
    };

    let task_json = fs::read_to_string(&task_file)?;
    let task: WorkerTask = serde_json::from_str(&task_json)?;

    let api_key = task.api_key.as_deref().unwrap_or(
        &std::env::var("OPENAI_API_KEY").unwrap_or_default()
    ).to_string();
    let base_url = task.base_url.as_deref().unwrap_or(
        &std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into())
    ).to_string();
    if api_key.len() < 5 { eprintln!("FATAL: no API key"); std::process::exit(1); }

    let client = LlmClient::openai(api_key, base_url, task.model.clone());
    let start = Instant::now();
    let max_duration = Duration::from_secs(std::env::var("WORKER_MAX_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(600));
    let mut tokens_used: usize = 0;
    let max_tokens: usize = std::env::var("WORKER_MAX_TOKENS").ok().and_then(|s| s.parse().ok()).unwrap_or(150000);

    // ── Phase 0: Decompose — Plan subtasks and define acceptance criteria ──

    let files_list = task.review_paths.join("\n  ");
    let session_context = task.session_map.as_deref().unwrap_or("");
    let has_session = !session_context.is_empty();

    let system = if task.system_prompt.is_empty() {
        format!("You are a {}. Work methodically like a senior professional. \
                 Decompose tasks, track progress, verify against criteria, deliver complete results.", task.profile_name)
    } else {
        task.system_prompt.clone()
    };

    let session_block = if has_session { format!("\n\nSESSION CONTEXT (other workers' progress):\n{session_context}") } else { String::new() };

    let decompose_prompt = format!(
        "You are a {role}. Your mission:\n\n\
         TASK: {task}\nSUBTASK: {subtask}\n\n\
         FILES TO REVIEW: {files}{session}\n\n\
         Before executing, you MUST:\n\
         1. Define 2-4 specific ACCEPTANCE CRITERIA (what 'done' means)\n\
         2. Decompose into 2-6 SUBTASKS (concrete steps to complete)\n\
         3. If session context shows other workers' work, consider it\n\n\
         Output as JSON:\n\
         {{\"acceptance_criteria\": [\"criterion 1\", ...], \
           \"subtasks\": [{{\"id\":1, \"description\":\"step 1\"}}, ...]}}\n\n\
         Only output valid JSON, no other text.",
        role = task.profile_name, task = task.task, subtask = task.subtask,
        files = files_list, session = session_block
    );

    let (plan_text, plan_tokens) = client.chat(&system, &decompose_prompt).await?;
    tokens_used += plan_tokens as usize;

    // Parse decomposition
    let plan: serde_json::Value = serde_json::from_str(&plan_text).unwrap_or_else(|_| {
        serde_json::json!({
            "acceptance_criteria": ["Complete the assigned review"],
            "subtasks": [{"id": 1, "description": task.subtask.clone()}]
        })
    });

    let acceptance_criteria: Vec<String> = plan["acceptance_criteria"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["Complete the task".into()]);

    let subtasks: Vec<SubTask> = plan["subtasks"].as_array()
        .map(|a| a.iter().enumerate().map(|(i, v)| SubTask {
            id: v["id"].as_u64().unwrap_or(i as u64 + 1) as usize,
            description: v["description"].as_str().unwrap_or("review").into(),
            status: "pending".into(),
            output: String::new(),
        }).collect())
        .unwrap_or_else(|| vec![SubTask { id: 1, description: task.subtask.clone(), status: "pending".into(), output: String::new() }]);

    let state_file = format!("/tmp/worker_state_{}.json", uuid::Uuid::now_v7().simple().to_string().chars().take(8).collect::<String>());
    let mut state = TaskState {
        goal: format!("{}: {}", task.task, task.subtask),
        role: task.profile_name.clone(),
        acceptance_criteria,
        subtasks,
        context: format!("Files available: {files_list}"),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    fs::write(&state_file, serde_json::to_string_pretty(&state).unwrap_or_default())?;

    eprintln!("[Plan] {} subtasks, {} criteria → {state_file}", state.subtasks.len(), state.acceptance_criteria.len());

    // ── Phase 1: Expert Execution Loop ──

    let mut cycles = 0;
    loop {
        let elapsed = start.elapsed();
        if elapsed > max_duration {
            write_reject(&task, "timeout", &format!("exceeded {}s", max_duration.as_secs()), &state);
            return Ok(());
        }
        if tokens_used >= max_tokens {
            write_reject(&task, "token_budget", &format!("exceeded {max_tokens} tokens"), &state);
            return Ok(());
        }

        cycles += 1;

        // Pick next subtask
        let next = state.subtasks.iter().position(|t| t.status == "pending");
        if next.is_none() && state.all_done() {
            break; // All done — proceed to final verification
        }
        if let Some(idx) = next {
            state.subtasks[idx].status = "in_progress".into();
        }

        fs::write(&state_file, serde_json::to_string_pretty(&state).unwrap_or_default())?;

        let session_tools = if has_session {
            "\n  SESSION TOOLS:\n  - get_worker_output(id) → read another worker's results\n  - get_artifact(id) → access artifact by ID\n  - get_instructions() → read original user instructions\n  - search_session(query) → find relevant info across session\n"
        } else { "" };

        let execute_prompt = format!(
            "CYCLE {cycle}\n\n{state_summary}\n\n\
             TOOLS AVAILABLE:\n\
             - read_file(path) — read source file contents\n\
             - list_dir(path) — list files in directory\n\
             - complete_task(id, output) — mark task as done with result\n\
             - update_context(info) — add findings to shared context\n\
             - add_task(description) — add new subtask if discovered{session_tools}\n\
             RULES:\n\
             1. Review the TASK LIST above — focus on current in_progress task [▶]\n\
             2. Execute the current task: use tools to read files, gather information\n\
             3. When a task is complete, output: COMPLETE_TASK: <id>: <result>\n\
             4. If you discover a new task is needed, output: ADD_TASK: <description>\n\
             5. If ALL tasks are done, output: ALL_DONE: <final verification against criteria>\n\
             6. NEVER output 'DONE' unless ALL subtasks are complete\n\
             7. Reference specific file paths and line numbers\n\
             8. Use session query tools to leverage other workers' findings\n\n\
             What is your next action?",
            cycle = cycles, state_summary = state.summary(), session_tools = session_tools
        );

        match client.chat(&system, &execute_prompt).await {
            Ok((text, tks)) => {
                tokens_used += tks as usize;
                let t = text.trim();

                if t.to_uppercase().starts_with("COMPLETE_TASK") || t.to_uppercase().starts_with("COMPLETE TASK") {
                    let rest = t.strip_prefix("COMPLETE_TASK:").or(t.strip_prefix("COMPLETE TASK:")).unwrap_or(t);
                    let parts: Vec<&str> = rest.trim().splitn(2, ':').collect();
                    if let Ok(id) = parts[0].trim().parse::<usize>() {
                        let result = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                        if let Some(st) = state.subtasks.iter_mut().find(|s| s.id == id) {
                            st.status = "done".into();
                            st.output = result;
                            eprintln!("  [✓] Task {id} complete");
                        }
                    }
                } else if t.to_uppercase().starts_with("ADD_TASK") || t.to_uppercase().starts_with("ADD TASK") {
                    let desc = t.strip_prefix("ADD_TASK:").or(t.strip_prefix("ADD TASK:")).unwrap_or(t).trim();
                    let new_id = state.subtasks.len() + 1;
                    state.subtasks.push(SubTask { id: new_id, description: desc.into(), status: "pending".into(), output: String::new() });
                    eprintln!("  [+] Task {new_id}: {desc}");
                } else if t.to_uppercase().starts_with("UPDATE_CONTEXT") || t.to_uppercase().starts_with("UPDATE CONTEXT") {
                    let ctx = t.strip_prefix("UPDATE_CONTEXT:").or(t.strip_prefix("UPDATE CONTEXT:")).unwrap_or(t).trim();
                    state.context.push_str(&format!("\n[C{cycles}]: {ctx}\n"));
                } else if t.to_uppercase().starts_with("ALL_DONE") || t.to_uppercase().starts_with("ALL DONE") {
                    eprintln!("  [Done] Worker declares all tasks complete");
                    break;
                } else if t.contains("NEED_FILE:") || t.contains("read_file(") {
                    // Extract file path and read it
                    let path = t.split("NEED_FILE:").nth(1)
                        .or_else(|| t.split("read_file(").nth(1).and_then(|s| s.split(')').next()))
                        .map(|s| s.trim().trim_matches('"').trim_matches('\''))
                        .unwrap_or("");
                    if !path.is_empty() {
                        match read_file(path).await {
                            Ok(content) => {
                                let truncated = truncate_at_char(&content, 6000);
                                state.context.push_str(&format!("\n=== {path} ===\n{truncated}\n"));
                                eprintln!("  [read] {path} ({} chars)", content.len());
                            }
                            Err(e) => {
                                state.context.push_str(&format!("\n[Read error: {path}: {e}]\n"));
                            }
                        }
                    }
                } else {
                    // Treat as general output — may contain multiple actions
                    // Append to context for next cycle
                    state.context.push_str(&format!("\n[C{cycles} output]: {}\n", t.chars().take(2000).collect::<String>()));
                }

                fs::write(&state_file, serde_json::to_string_pretty(&state).unwrap_or_default())?;
            }
            Err(e) => {
                eprintln!("[LLM error cycle {cycles}]: {e}");
                write_reject(&task, "error", &format!("LLM error: {e}"), &state);
                std::process::exit(1);
            }
        }
    }

    // ── Phase 2: Final Verification ──

    let verify_prompt = format!(
        "{state_summary}\n\n\
         ALL TASKS COMPLETE. Verify against ACCEPTANCE CRITERIA:\n\
         {criteria}\n\n\
         For each criterion, state: PASS: <evidence> or FAIL: <reason>.\n\
         Then: FINAL: PASS (all criteria met) or FINAL: FAIL <remaining issues>.\n\
         Include a comprehensive summary of findings.",
        state_summary = state.summary(),
        criteria = state.acceptance_criteria.iter().enumerate()
            .map(|(i,c)| format!("  {}. {c}", i+1)).collect::<Vec<_>>().join("\n")
    );

    match client.chat(&system, &verify_prompt).await {
        Ok((text, _)) => {
            let passed = text.to_uppercase().contains("FINAL: PASS");
            let summary = if text.len() > 15000 { format!("{}...", &text[..15000]) } else { text.clone() };
            let out = serde_json::json!({
                "success": passed,
                "action": if passed { "done" } else { "failed_criteria" },
                "profile": task.profile_name,
                "model": task.model,
                "cycles": cycles,
                "tokens": tokens_used,
                "output": summary,
                "subtasks_completed": state.subtasks.iter().filter(|t| t.status == "done").count(),
                "subtasks_total": state.subtasks.len(),
                "acceptance_criteria": state.acceptance_criteria,
            });
            write_output(&task, &out);
        }
        Err(e) => {
            write_reject(&task, "error", &format!("Verification failed: {e}"), &state);
        }
    }

    Ok(())
}

fn truncate_at_char(s: &str, max: usize) -> String {
    s.char_indices().nth(max).map(|(i, _)| s[..i].to_string()).unwrap_or_else(|| s.to_string())
}

fn write_output(task: &WorkerTask, output: &serde_json::Value) {
    let out_path = task.output_file.clone().unwrap_or_else(|| format!("/tmp/worker_output_{}.json", uuid::Uuid::now_v7()));
    let _ = fs::write(&out_path, serde_json::to_string_pretty(output).unwrap_or_default());
    println!("DONE: {out_path}");
}

fn write_reject(task: &WorkerTask, reason: &str, detail: &str, state: &TaskState) {
    let out = serde_json::json!({
        "success": false, "action": "reject", "reason": reason, "detail": detail,
        "subtasks_done": state.subtasks.iter().filter(|t| t.status == "done").count(),
        "subtasks_total": state.subtasks.len(),
    });
    write_output(task, &out);
}
