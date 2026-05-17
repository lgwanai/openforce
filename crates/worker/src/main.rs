use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::Deserialize;
use std::fs;

/// Worker binary — runs as an independent OS process.
/// Spawned by NodeDaemon via `worker --task-file /tmp/task.json`
/// Each worker process = one sandbox instance.

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

    if api_key.len() < 5 {
        eprintln!("FATAL: no API key");
        std::process::exit(1);
    }

    let client = LlmClient::openai(api_key, base_url, task.model.clone());

    let files_section = task.files_text.as_deref().unwrap_or("");
    let prompt = format!(
        "Task: {}\nSubtask: {}\nProfile: {}\n\nSource Code:\n{}\n\nComplete the subtask and output results. Reference specific files and line numbers.",
        task.task, task.subtask, task.profile_name, files_section
    );

    let system = if task.system_prompt.is_empty() {
        format!("Worker executing: {}", task.subtask)
    } else {
        task.system_prompt.clone()
    };

    match client.chat(&system, &prompt).await {
        Ok((text, tokens)) => {
            let output = serde_json::json!({
                "success": true,
                "profile": task.profile_name,
                "model": task.model,
                "tokens": tokens,
                "output": text,
            });
            let out_path = task.output_file.unwrap_or_else(|| {
                format!("/tmp/worker_output_{}.json", uuid::Uuid::now_v7())
            });
            fs::write(&out_path, serde_json::to_string_pretty(&output)?)?;
            println!("OK: {out_path}");
        }
        Err(e) => {
            eprintln!("Worker error: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}
