use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{Duration, Instant};

// ── Agent Memory Model ──

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubTask {
    id: usize,
    description: String,
    status: String,
    output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Decision {
    cycle: usize,
    action: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentMemory {
    // LAYER 1: IDENTITY
    role: String,
    goal: String,
    acceptance_criteria: Vec<String>,

    // LAYER 2: WORKING MEMORY
    subtasks: Vec<SubTask>,
    recent_decisions: Vec<Decision>,
    key_findings: Vec<String>,

    // Track state
    started_at: String,
    files_available: Vec<String>,
}

impl AgentMemory {
    fn build_context(&self) -> String {
        let mut ctx = String::new();

        // LAYER 1: IDENTITY
        ctx.push_str(&format!("ROLE: {}\nGOAL: {}\n\n", self.role, self.goal));
        ctx.push_str("ACCEPTANCE CRITERIA:\n");
        for (i, c) in self.acceptance_criteria.iter().enumerate() {
            ctx.push_str(&format!("  {}. {}\n", i + 1, c));
        }

        // LAYER 2: WORKING MEMORY
        ctx.push_str("\n── SUBTASKS ──\n");
        for t in &self.subtasks {
            let icon = match t.status.as_str() {
                "done" => "✓", "in_progress" => "▶", "blocked" => "✗", _ => "○"
            };
            ctx.push_str(&format!("  [{icon}] {}. {}\n", t.id, t.description));
            if !t.output.is_empty() {
                ctx.push_str(&format!("       → {}\n", t.output));
            }
        }
        let done = self.subtasks.iter().filter(|t| t.status == "done").count();
        ctx.push_str(&format!("\nProgress: {}/{}\n", done, self.subtasks.len()));

        ctx.push_str("\n── KEY FINDINGS ──\n");
        for f in &self.key_findings {
            ctx.push_str(&format!("  • {}\n", f));
        }
        if self.key_findings.is_empty() {
            ctx.push_str("  (none yet)\n");
        }

        ctx.push_str("\n── RECENT DECISIONS ──\n");
        for d in self.recent_decisions.iter().rev().take(5) {
            ctx.push_str(&format!("  [C{}] {}: {}\n", d.cycle, d.action, d.detail));
        }

        // LAYER 3: Available files (paths only, not content)
        if !self.files_available.is_empty() {
            ctx.push_str(&format!("\n── AVAILABLE FILES ({} total) ──\n", self.files_available.len()));
            for f in self.files_available.iter().take(30) {
                ctx.push_str(&format!("  {}\n", f));
            }
            if self.files_available.len() > 30 {
                ctx.push_str(&format!("  ... and {} more\n", self.files_available.len() - 30));
            }
        }

        ctx
    }

    fn estimate_tokens(&self) -> usize {
        self.build_context().len() / 4
    }

    /// Compress LAYER 2: merge old decisions, dedup findings
    fn compress_if_needed(&mut self) {
        let tokens = self.estimate_tokens();
        if tokens < 700_000 { return; }

        // Merge decisions older than 10 cycles into summary
        let recent: Vec<Decision> = self.recent_decisions.iter()
            .rev().take(5).cloned().collect::<Vec<_>>().into_iter().rev().collect();
        let old_summary = format!("[Earlier: {} decisions completed]", self.recent_decisions.len().saturating_sub(5));
        self.recent_decisions = recent;
        if !old_summary.contains("0 decisions") {
            self.recent_decisions.insert(0, Decision { cycle: 0, action: "SUMMARY".into(), detail: old_summary });
        }

        // Keep only top 5 findings by recency
        if self.key_findings.len() > 5 {
            self.key_findings = self.key_findings.iter().rev().take(5).cloned().collect::<Vec<_>>().into_iter().rev().collect();
        }
    }

    fn add_decision(&mut self, cycle: usize, action: &str, detail: &str) {
        self.recent_decisions.push(Decision { cycle, action: action.into(), detail: detail.into() });
        if self.recent_decisions.len() > 20 {
            self.recent_decisions.remove(0);
        }
    }

    fn add_finding(&mut self, finding: &str) {
        let summary: String = finding.to_string();
        if !self.key_findings.contains(&summary) {
            self.key_findings.push(summary);
        }
    }

    fn all_done(&self) -> bool {
        self.subtasks.iter().all(|t| t.status == "done")
    }
}

// ── Worker Task Input ──

#[derive(Debug, Deserialize)]
struct WorkerTask {
    task: String,
    subtask: String,
    profile_name: String,
    #[serde(default)]
    provider: String,
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

fn extract_json(s: &str) -> Option<&str> {
    if let Some(start) = s.find('{') {
        let end = s.rfind('}')?;
        Some(&s[start..=end])
    } else { None }
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
        &std::env::var("API_KEY").unwrap_or_default()
    ).to_string();
    let base_url = task.base_url.as_deref().unwrap_or(
        &std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into())
    ).to_string();
    if api_key.len() < 5 { eprintln!("FATAL: no API key"); std::process::exit(1); }

    let provider = if task.provider.is_empty() { "openai" } else { &task.provider };
    let client = match provider {
        "anthropic" => LlmClient::anthropic(api_key, Some(base_url)).with_model(&task.model),
        _ => LlmClient::openai(api_key, base_url, task.model.clone())
    };

    let start = Instant::now();
    let max_duration = Duration::from_secs(std::env::var("WORKER_MAX_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(1200));
    let max_cycles: usize = std::env::var("WORKER_MAX_CYCLES").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
    let max_no_progress: usize = std::env::var("WORKER_MAX_NO_PROGRESS").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let mut tokens_used: usize = 0;
    let max_tokens: usize = std::env::var("WORKER_MAX_TOKENS").ok().and_then(|s| s.parse().ok()).unwrap_or(150000);

    let system = if task.system_prompt.is_empty() {
        format!("You are a {}. You are an autonomous agent. Manage your own context: request files when needed via READ, declare completion via DONE, declare all done via ALL_DONE.", task.profile_name)
    } else {
        task.system_prompt.clone()
    };

    // ── Phase 0: Decompose ──

    let decompose_prompt = format!(
        "Role: {role}\nTask: {task}\nSubtask: {subtask}\n\n\
         Available files ({n} total):\n{files}\n\n\
         Define 2-4 acceptance criteria + 2-4 subtasks. Output JSON only:\n\
         {{\"acceptance_criteria\":[\"...\"],\"subtasks\":[{{\"id\":1,\"description\":\"...\"}}]}}",
        role = task.profile_name, task = task.task, subtask = task.subtask,
        n = task.review_paths.len(),
        files = task.review_paths.iter().take(40).map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n")
    );

    let (plan_text, plan_tokens) = client.chat(&system, &decompose_prompt).await?;
    tokens_used += plan_tokens as usize;

    let plan: serde_json::Value = serde_json::from_str(&plan_text).unwrap_or_else(|_| {
        serde_json::from_str(extract_json(&plan_text).unwrap_or("{}")).unwrap_or_else(|_| {
            serde_json::json!({
                "acceptance_criteria": ["Complete the review"],
                "subtasks": [{"id": 1, "description": task.subtask.clone()}]
            })
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
    let mut memory = AgentMemory {
        role: task.profile_name.clone(),
        goal: format!("{}: {}", task.task, task.subtask),
        acceptance_criteria,
        subtasks,
        recent_decisions: vec![],
        key_findings: vec![],
        started_at: chrono::Utc::now().to_rfc3339(),
        files_available: task.review_paths.clone(),
    };

    eprintln!("[Agent] {} subtasks, {} criteria, {} files available",
        memory.subtasks.len(), memory.acceptance_criteria.len(), memory.files_available.len());

    // ── Phase 1: Agent Execution Loop ──

    let mut cycles = 0;
    let mut last_progress = 0usize;
    let mut active_file_content: Option<(String, String)> = None; // (path, content)

    loop {
        let elapsed = start.elapsed();
        let done_count = memory.subtasks.iter().filter(|t| t.status == "done").count();

        if elapsed > max_duration {
            let out = serde_json::json!({
                "success": false, "action": "timeout", "reason": "max_duration",
                "detail": format!("exceeded {}s, {}/{} done", max_duration.as_secs(), done_count, memory.subtasks.len()),
                "subtasks_done": done_count, "subtasks_total": memory.subtasks.len(),
            });
            write_output(&task, &out);
            return Ok(());
        }
        if tokens_used >= max_tokens {
            let out = serde_json::json!({
                "success": false, "action": "timeout", "reason": "token_budget",
                "detail": format!("exceeded {max_tokens} tokens"),
                "subtasks_done": done_count, "subtasks_total": memory.subtasks.len(),
            });
            write_output(&task, &out);
            return Ok(());
        }
        if cycles >= max_cycles {
            if done_count > 0 {
                eprintln!("  [Agent] {} cycles, {}/{} done — finalizing", cycles, done_count, memory.subtasks.len());
                break;
            }
            let out = serde_json::json!({
                "success": false, "action": "stalled", "reason": "max_cycles",
                "detail": format!("exceeded {max_cycles} cycles, 0 progress"),
                "subtasks_done": 0, "subtasks_total": memory.subtasks.len(),
            });
            write_output(&task, &out);
            return Ok(());
        }

        cycles += 1;

        // Pick next pending subtask
        if let Some(idx) = memory.subtasks.iter().position(|t| t.status == "pending") {
            memory.subtasks[idx].status = "in_progress".into();
        }
        if memory.all_done() { break; }

        // Check for no-progress stall
        if done_count > last_progress {
            last_progress = done_count;
        }
        if cycles.saturating_sub(last_progress * 3) > max_no_progress && last_progress < memory.subtasks.len() {
            // Force conclusion — build fresh context including active file
            let mut force_ctx = memory.build_context();
            if let Some((ref path, ref content)) = active_file_content {
                force_ctx.push_str(&format!("\n── READING: {path} ──\n{content}\n"));
            }
            let force_prompt = format!(
                "{force_ctx}\n\n── FORCE CONCLUSION ──\n\
                 You have NOT marked any task DONE in {stall} cycles.\n\
                 Give your FINAL conclusion NOW.\n\
                 Format: DONE <id>: <result>\n\
                 If you truly cannot complete: STALLED: <reason>",
                force_ctx = force_ctx, stall = cycles.saturating_sub(last_progress * 3)
            );
            match client.chat(&system, &force_prompt).await {
                Ok((text, tks)) => {
                    tokens_used += tks as usize;
                    let t = text.trim();
                    if t.to_uppercase().starts_with("DONE") {
                        let forced_id = {
                            let st = memory.subtasks.iter_mut().find(|s| s.status == "in_progress");
                            st.map(|s| { s.status = "done".into(); s.output = t.to_string(); s.id })
                        };
                        if let Some(fid) = forced_id {
                            memory.add_decision(cycles, "FORCED_DONE", &format!("Task {} completed after stall", fid));
                            eprintln!("  [✓] Task {fid} done (forced)");
                            last_progress = memory.subtasks.iter().filter(|t| t.status == "done").count();
                            continue;
                        }
                    } else if t.to_uppercase().starts_with("STALLED") {
                        let out = serde_json::json!({
                            "success": false, "action": "stalled", "reason": "agent_declared_stalled",
                            "detail": t.to_string(),
                            "subtasks_done": done_count, "subtasks_total": memory.subtasks.len(),
                        });
                        write_output(&task, &out);
                        return Ok(());
                    }
                }
                Err(e) => {
                    let out = serde_json::json!({
                        "success": false, "action": "stalled", "reason": "llm_error",
                        "detail": format!("{e}"),
                        "subtasks_done": done_count, "subtasks_total": memory.subtasks.len(),
                    });
                    write_output(&task, &out);
                    return Ok(());
                }
            }
        }

        memory.compress_if_needed();

        // Assemble context: build from memory (files are paths only, no content loaded)
        let mut ctx = memory.build_context();

        // Add active file content if any
        if let Some((ref path, ref content)) = active_file_content {
            ctx.push_str(&format!("\n── READING: {path} ──\n{content}\n"));
        }

        let execute_prompt = format!(
            "{ctx}\n\n── CYCLE {cycle}/{max_cycles} ──\n\
             Actions (put ONE on the LAST line):\n\
               DONE <id>: <result>    — mark subtask complete\n\
               READ: <file_path>      — read a file from AVAILABLE FILES\n\
               FINDING: <text>        — record a key finding\n\
               ALL_DONE               — all subtasks complete\n\
               CONTINUE: <next step>  — still working\n\
             \nYOUR RESPONSE:",
            cycle = cycles, max_cycles = max_cycles
        );

        match client.chat(&system, &execute_prompt).await {
            Ok((text, tks)) => {
                tokens_used += tks as usize;
                let t = text.trim();
                let last_line = t.lines().last().unwrap_or("").trim();
                let body = t.lines().rev().skip(1).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                let upper = last_line.to_uppercase();

                if upper.starts_with("DONE ") || upper.starts_with("DONE:") {
                    let rest = last_line.strip_prefix("DONE ").or(last_line.strip_prefix("DONE:")).unwrap_or("");
                    let task_id = rest.trim().split(|c: char| c == ':' || c == ' ').next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                    let result = rest.trim().splitn(2, |c: char| c == ':' || c == ' ').nth(1).map(|s| s.trim().to_string()).unwrap_or_else(|| body.to_string());
                    if let Some(st) = memory.subtasks.iter_mut().find(|s| s.id == task_id) {
                        st.status = "done".into();
                        st.output = result.clone();
                    } else if let Some(st) = memory.subtasks.iter_mut().find(|s| s.status == "in_progress") {
                        st.status = "done".into();
                        st.output = result.clone();
                    }
                    memory.add_decision(cycles, "DONE", &format!("Task {}: {}", task_id, result));
                    eprintln!("  [✓] Task {task_id} done");
                    last_progress = memory.subtasks.iter().filter(|t| t.status == "done").count();
                    active_file_content = None; // Clear file focus after completing a task
                } else if upper.starts_with("ALL_DONE") || upper.starts_with("ALL DONE") {
                    eprintln!("  [Done] Agent declares all tasks complete");
                    break;
                } else if upper.starts_with("READ:") || upper.starts_with("READ ") {
                    let path = last_line.strip_prefix("READ:").or(last_line.strip_prefix("READ ")).unwrap_or("").trim();
                    // Try matching against available files (partial match)
                    let matched = memory.files_available.iter()
                        .find(|f| f.ends_with(path) || f.contains(path))
                        .cloned()
                        .unwrap_or_else(|| path.to_string());
                    match read_file(&matched).await {
                        Ok(content) => {
                            memory.add_decision(cycles, "READ", &format!("{} ({} chars)", matched, content.len()));
                            active_file_content = Some((matched.clone(), content));
                            eprintln!("  [read] {matched}");
                        }
                        Err(e) => {
                            memory.add_decision(cycles, "READ_ERR", &format!("{matched}: {e}"));
                            eprintln!("  [err] {e}");
                        }
                    }
                } else if upper.starts_with("FINDING:") {
                    let finding = last_line.strip_prefix("FINDING:").unwrap_or("").trim();
                    memory.add_finding(finding);
                    memory.add_decision(cycles, "FINDING", finding);
                    eprintln!("  [!] Finding recorded");
                } else if upper.starts_with("CONTINUE:") || upper.starts_with("CONTINUE ") {
                    let step = last_line.strip_prefix("CONTINUE:").or(last_line.strip_prefix("CONTINUE ")).unwrap_or("").trim();
                    memory.add_decision(cycles, "CONTINUE", step);
                    eprintln!("  [→] {step}");
                } else {
                    // No action keyword found — check body for DONE
                    let mut handled = false;
                    for bline in t.lines() {
                        let bu = bline.trim().to_uppercase();
                        if bu.starts_with("DONE ") {
                            let rest = bline.trim().strip_prefix("DONE ").unwrap_or("");
                            let done_id = {
                                let st = memory.subtasks.iter_mut().find(|s| s.status == "in_progress");
                                st.map(|s| { s.status = "done".into(); s.output = rest.to_string(); s.id })
                            };
                            if let Some(did) = done_id {
                                let summary: String = rest.to_string();
                                memory.add_decision(cycles, "DONE", &format!("Task {did}: {summary}"));
                                eprintln!("  [✓] Task {did} done (from body)");
                                last_progress = memory.subtasks.iter().filter(|t| t.status == "done").count();
                                handled = true;
                                break;
                            }
                        }
                    }
                    if !handled {
                        // LLM wrote analysis without action — treat as working
                        let excerpt: String = t.to_string();
                        memory.add_decision(cycles, "ANALYSIS", &excerpt);
                        // Auto-extract findings from analysis text
                        if t.len() > 300 {
                            memory.add_finding(t);
                        }
                        eprintln!("  [~] Analysis ({} chars)", t.len());
                    }
                }

                fs::write(&state_file, serde_json::to_string_pretty(&memory).unwrap_or_default())?;
            }
            Err(e) => {
                eprintln!("[LLM error cycle {cycles}]: {e}");
                let out = serde_json::json!({
                    "success": false, "action": "error", "reason": "llm_error",
                    "detail": format!("{e}"),
                    "subtasks_done": memory.subtasks.iter().filter(|t| t.status == "done").count(),
                    "subtasks_total": memory.subtasks.len(),
                });
                write_output(&task, &out);
                std::process::exit(1);
            }
        }
    }

    // ── Phase 2: Final Verification ──

    let verify_prompt = format!(
        "{ctx}\n\nFINAL VERIFICATION: Evaluate each criterion. Output: PASS/FAIL + evidence. End with: FINAL: PASS|FAIL",
        ctx = memory.build_context()
    );

    match client.chat(&system, &verify_prompt).await {
        Ok((text, _)) => {
            let passed = text.to_uppercase().contains("FINAL: PASS");
            let summary = text.clone();
            let done_count = memory.subtasks.iter().filter(|t| t.status == "done").count();
            let out = serde_json::json!({
                "success": passed,
                "action": if passed { "completed" } else { "failed_criteria" },
                "profile": task.profile_name,
                "model": task.model,
                "cycles": cycles,
                "tokens": tokens_used,
                "output": summary,
                "subtasks_completed": done_count,
                "subtasks_total": memory.subtasks.len(),
                "acceptance_criteria": memory.acceptance_criteria,
            });
            write_output(&task, &out);
        }
        Err(e) => {
            let out = serde_json::json!({
                "success": false, "action": "error", "reason": "verification_failed",
                "detail": format!("{e}"),
                "subtasks_done": memory.subtasks.iter().filter(|t| t.status == "done").count(),
                "subtasks_total": memory.subtasks.len(),
            });
            write_output(&task, &out);
        }
    }

    Ok(())
}

fn write_output(task: &WorkerTask, output: &serde_json::Value) {
    let out_path = task.output_file.clone().unwrap_or_else(|| format!("/tmp/worker_output_{}.json", uuid::Uuid::now_v7()));
    let _ = fs::write(&out_path, serde_json::to_string_pretty(output).unwrap_or_default());
    println!("DONE: {out_path}");
}
