use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct WorkerTask {
    task: String,
    subtask: String,
    profile_name: String,
    model: String,
    system_prompt: String,
    workspace: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    output_file: Option<String>,
    files_text: Option<String>,
}

#[derive(Debug)]
enum ReActAction {
    Continue(String),
    Reject(String),
    Done(String),
}

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
    let system = if task.system_prompt.is_empty() {
        format!("You are a {}. Execute tasks with precision.", task.profile_name)
    } else {
        task.system_prompt.clone()
    };

    // ── Phase 1: Task Validation ──
    let files = task.files_text.as_deref().unwrap_or("");
    let has_context = files.len() > 200;
    let validate_prompt = format!(
        "You received a task:\n\
         TASK: {}\n\
         SUBTASK: {}\n\
         ROLE: {}\n\
         SOURCE FILES PROVIDED: {} bytes\n\n\
         Evaluate if you can execute this task well.\n\
         Reply EXACTLY one of:\n\
         - READY: <one sentence why you can proceed>\n\
         - REJECT: <specific reason — missing context, too vague, not enough source, etc>",
        task.task, task.subtask, task.profile_name,
        if has_context { files.len().to_string() } else { "NONE".to_string() }
    );

    match client.chat(&system, &validate_prompt).await {
        Ok((text, _)) => {
            if text.trim().to_uppercase().starts_with("REJECT") {
                let reason = text.trim().strip_prefix("REJECT:").unwrap_or(&text).trim();
                eprintln!("[REJECTED] {reason}");
                let output = serde_json::json!({"success": false, "action": "reject", "reason": reason});
                write_output(&task, &output);
                return Ok(());
            }
        }
        Err(e) => { eprintln!("Validation failed: {e}"); std::process::exit(1); }
    }

    // ── Phase 2: ReAct Loop ──
    let max_cycles = 3;
    let mut context = String::new();

    for cycle in 0..max_cycles {
        let act_prompt = format!(
            "CYCLE {}/{}\n\nTASK: {}\nSUBTASK: {}\nROLE: {}\n\n{}\n\nPREVIOUS CONTEXT:\n{}\n\n\
             Execute this cycle. Output format:\n\
             - If you have enough info and can produce results: DONE: <your detailed output>\n\
             - If you need more info from source: CONTINUE: <what you need to check next>\n\
             - If the task is impossible: REJECT: <why>",
            cycle + 1, max_cycles, task.task, task.subtask, task.profile_name, files, context
        );

        match client.chat(&system, &act_prompt).await {
            Ok((text, _)) => {
                let action = if text.trim().to_uppercase().starts_with("DONE") {
                    ReActAction::Done(text.trim().strip_prefix("DONE:").unwrap_or(&text).trim().to_string())
                } else if text.trim().to_uppercase().starts_with("REJECT") {
                    ReActAction::Reject(text.trim().strip_prefix("REJECT:").unwrap_or(&text).trim().to_string())
                } else {
                    ReActAction::Continue(text.clone())
                };

                match action {
                    ReActAction::Done(output) => {
                        let out = serde_json::json!({
                            "success": true, "action": "done", "profile": task.profile_name,
                            "model": task.model, "cycles": cycle + 1, "output": output,
                        });
                        write_output(&task, &out);
                        return Ok(());
                    }
                    ReActAction::Reject(reason) => {
                        eprintln!("[REJECTED cycle {cycle}] {reason}");
                        let out = serde_json::json!({
                            "success": false, "action": "reject", "reason": reason, "cycle": cycle,
                        });
                        write_output(&task, &out);
                        return Ok(());
                    }
                    ReActAction::Continue(insight) => {
                        context.push_str(&format!("\n[Cycle {cycle} insight]: {insight}\n"));
                    }
                }
            }
            Err(e) => {
                eprintln!("Worker LLM error cycle {cycle}: {e}");
                if cycle + 1 >= max_cycles {
                    let out = serde_json::json!({"success": false, "action": "error", "reason": format!("{e}")});
                    write_output(&task, &out);
                    std::process::exit(1);
                }
                context.push_str(&format!("\n[Cycle {cycle} error]: {e}\n"));
            }
        }
    }

    let out = serde_json::json!({"success": false, "action": "exhausted", "reason": "max cycles reached"});
    write_output(&task, &out);
    Ok(())
}

fn write_output(task: &WorkerTask, output: &serde_json::Value) {
    let out_path = task.output_file.clone().unwrap_or_else(|| {
        format!("/tmp/worker_output_{}.json", uuid::Uuid::now_v7())
    });
    let _ = fs::write(&out_path, serde_json::to_string_pretty(output).unwrap_or_default());
    println!("DONE: {out_path}");
}
