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
    api_key: Option<String>,
    base_url: Option<String>,
    output_file: Option<String>,
    /// Files/directories to review (e.g. ["crates/domain/src/lease.rs", "crates/mtls/"])
    review_paths: Vec<String>,
    /// gRPC endpoint for remote file access (e.g. "127.0.0.1:50053")
    project_tools_addr: Option<String>,
}

/// File reading tool. Tries gRPC first, falls back to local disk.
async fn read_file(path: &str, _tools_addr: Option<&str>) -> Result<String, String> {
    // For now: local disk access. gRPC client to Project Tools will be wired in Phase 5.
    // The architecture is ready — `project_tools_addr` is passed in the task,
    // and a `ProjectToolServiceClient` from the proto crate can be connected here.
    if let Some(_addr) = _tools_addr {
        // TODO: connect gRPC client to ReadProjectFile
        // let mut client = ProjectToolServiceClient::connect(format!("http://{_addr}")).await?;
        // let resp = client.read_project_file(ReadProjectFileRequest { ... }).await?;
        // return Ok(resp.content);
    }
    fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))
}

/// List files in a directory (local fallback).
async fn list_dir(dir: &str) -> Result<Vec<String>, String> {
    let mut files = vec![];
    for entry in std::fs::read_dir(dir).map_err(|e| format!("readdir {dir}: {e}"))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let path = entry.path();
        if path.is_file() {
            files.push(path.display().to_string());
        }
    }
    Ok(files)
}

#[derive(Debug)]
enum ReActAction {
    Done(String),
    Reject(String),
    NeedFile(String),
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

    let files_list = task.review_paths.join("\n  ");
    let tools_available = format!(
        "Available tools:\n  read_file(path) — read a file by path\n  list_dir(path) — list files in directory\n\n\
         Files to review:\n  {files_list}"
    );

    let system = if task.system_prompt.is_empty() {
        format!("You are a {}. Use tools to read files and produce a thorough review. Reference specific files and line numbers.", task.profile_name)
    } else {
        format!("{}\n\nUse the read_file tool to access source code. Reference specific files and line numbers in your output.", task.system_prompt)
    };

    // ── Phase 1: Task Validation ──
    let validate_prompt = format!(
        "Task: {}\nSubtask: {}\nFiles assigned: {}\n\n\
         Evaluate if you can execute this review. You have access to read_file() and list_dir() tools.\n\
         Reply EXACTLY: READY: <brief plan>  OR  REJECT: <specific reason>",
        task.task, task.subtask, files_list
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

    // ── Phase 2: ReAct Loop with Tool Use ──
    let max_cycles = 4;
    let mut context = String::new();
    let mut files_read: Vec<String> = vec![];

    for cycle in 0..max_cycles {
        let act_prompt = format!(
            "CYCLE {}/{}\n\nTASK: {}\nSUBTASK: {}\n\nTOOLS:\n{}\n\n\
             FILES READ SO FAR:\n{}\n\nCONTEXT:\n{}\n\n\
             Decide your next action. Choose EXACTLY ONE format:\n\
             - To read a file: NEED_FILE: <path>\n\
             - To produce final result: DONE: <detailed review output>\n\
             - If task is impossible: REJECT: <why>",
            cycle + 1, max_cycles, task.task, task.subtask, tools_available,
            files_read.join("\n  "), context
        );

        match client.chat(&system, &act_prompt).await {
            Ok((text, _)) => {
                let t = text.trim();
                let action = if t.to_uppercase().starts_with("NEED_FILE") {
                    let path = t.strip_prefix("NEED_FILE:").unwrap_or(t).trim().to_string();
                    ReActAction::NeedFile(path)
                } else if t.to_uppercase().starts_with("DONE") {
                    ReActAction::Done(t.strip_prefix("DONE:").unwrap_or(t).trim().to_string())
                } else if t.to_uppercase().starts_with("REJECT") {
                    ReActAction::Reject(t.strip_prefix("REJECT:").unwrap_or(t).trim().to_string())
                } else {
                    // Try to interpret as a file read request
                    if t.lines().count() <= 2 && (t.contains('/') || t.contains(".rs") || t.contains(".toml")) {
                        ReActAction::NeedFile(t.lines().next().unwrap_or(t).trim().to_string())
                    } else {
                        ReActAction::Done(t.to_string())
                    }
                };

                match action {
                    ReActAction::NeedFile(path) => {
                        let tools_addr = task.project_tools_addr.as_deref();
                        match read_file(&path, tools_addr).await {
                            Ok(content) => {
                                let truncated = truncate_at_char(&content, 8000);
                                context.push_str(&format!("\n=== {} ===\n{}\n", path, truncated));
                                files_read.push(path);
                            }
                            Err(e) => {
                                context.push_str(&format!("\n[Read error for {path}: {e}]\n"));
                            }
                        }
                    }
                    ReActAction::Done(output) => {
                        let out = serde_json::json!({
                            "success": true, "action": "done", "profile": task.profile_name,
                            "model": task.model, "cycles": cycle + 1, "output": output,
                            "files_read": files_read,
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
                }
            }
            Err(e) => {
                context.push_str(&format!("\n[Cycle {cycle} error]: {e}\n"));
                if cycle + 1 >= max_cycles {
                    let out = serde_json::json!({"success": false, "action": "error", "reason": format!("{e}")});
                    write_output(&task, &out);
                    std::process::exit(1);
                }
            }
        }
    }

    let out = serde_json::json!({"success": false, "action": "exhausted", "reason": "max cycles"});
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

fn truncate_at_char(s: &str, max_chars: usize) -> String {
    s.char_indices().nth(max_chars).map(|(i, _)| s[..i].to_string()).unwrap_or_else(|| s.to_string())
}
