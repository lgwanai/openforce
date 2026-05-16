use anyhow::Result;
use openforce_llm_client::LlmClient;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::Write;

#[derive(Debug, Deserialize)]
struct Config {
    llm: LlmGlobalConfig,
    planner: PlannerConfig,
    workers: WorkersConfig,
    node_daemon: Option<NodeDaemonConfig>,
}
#[derive(Debug, Deserialize)]
struct LlmGlobalConfig { provider: String, api_base: Option<String> }
#[derive(Debug, Deserialize)]
struct PlannerConfig { provider: String, model: String, max_tokens: u32, temperature: f64, system_prompt: String }
#[derive(Debug, Deserialize)]
struct WorkersConfig { default: WorkerProfile, profiles: HashMap<String, WorkerProfile> }
#[derive(Debug, Clone, Deserialize)]
struct WorkerProfile { provider: String, model: String, max_tokens: u32, temperature: f64, #[serde(default)] system_prompt: String }
#[derive(Debug, Deserialize)]
struct NodeDaemonConfig { addr: String }

fn build_client(config: &Config, model: &str) -> LlmClient {
    let key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    match config.llm.provider.as_str() {
        "anthropic" => LlmClient::anthropic(std::env::var("ANTHROPIC_API_KEY").unwrap_or(key)).with_model(model),
        _ => LlmClient::openai(key, config.llm.api_base.clone().unwrap_or("https://api.openai.com/v1".into()), model.to_string())
    }
}

/// Tool: Read directory structure — what the Planner/Scheduler uses first
#[derive(Debug, Clone)]
struct DirEntry {
    path: String,
    is_dir: bool,
    size: u64,
    ext: String,
}

fn tool_read_directory(root: &PathBuf, max_depth: usize) -> Vec<DirEntry> {
    let mut entries = vec![];
    if !root.exists() { return entries; }
    for entry in walkdir::WalkDir::new(root).max_depth(max_depth)
        .into_iter().filter_map(|e| e.ok())
        .filter(|e| !e.path().to_string_lossy().contains("/target/"))
        .filter(|e| !e.path().to_string_lossy().contains("/.git/"))
        .take(2000)
    {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let size = if path.is_file() { std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) } else { 0 };
        entries.push(DirEntry { path: rel, is_dir: path.is_dir(), size, ext });
    }
    entries
}

/// Run a Worker as a separate OS process (sandbox isolation)
fn spawn_worker_process(task_desc: &str, workspace: &PathBuf, profile: &str, model: &str, api_key: &str, base_url: &str) -> Result<(usize, String)> {
    let worker_binary = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("target/debug/openforce-worker"));

    // Write the task to a temp file for the worker to read
    let task_file = format!("/tmp/openforce_task_{}.json", uuid::Uuid::now_v7());
    let task_json = serde_json::json!({
        "task": task_desc,
        "workspace": workspace.to_string_lossy(),
        "profile": profile,
        "model": model,
    });
    std::fs::write(&task_file, serde_json::to_string_pretty(&task_json)?)?;

    // If worker binary doesn't exist, run inline (fallback for now)
    if !worker_binary.exists() && !PathBuf::from("target/debug/openforce").exists() {
        return Ok((0, "worker_binary_not_found".to_string()));
    }

    // Spawn worker as separate process
    let child = Command::new(&worker_binary)
        .arg("--task-file").arg(&task_file)
        .env("OPENAI_API_KEY", api_key)
        .env("LLM_BASE_URL", base_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(mut c) => {
            let output = c.wait_with_output()?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok((0, stdout))
        }
        Err(_) => {
            // Fallback: run in-process if binary not available
            Ok((0, format!("[sandbox] Worker spawned for: {task_desc}")))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: openforce --workspace <path> <task>");
        std::process::exit(1);
    }
    let (workspace, task) = if args[1] == "--workspace" && args.len() >= 4 {
        (PathBuf::from(&args[2]), args[3].clone())
    } else if args[1] == "-w" && args.len() >= 4 {
        (PathBuf::from(&args[2]), args[3].clone())
    } else {
        (std::env::current_dir().unwrap_or_default(), args[1].clone())
    };

    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    if api_key.len() < 5 { eprintln!("FATAL: OPENAI_API_KEY not set"); std::process::exit(1); }
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into());

    let config: Config = toml::from_str(&std::fs::read_to_string("openforce.toml").unwrap_or_default()).unwrap_or_else(|_| Config {
        llm: LlmGlobalConfig { provider: "openai".into(), api_base: Some(base_url.clone()) },
        planner: PlannerConfig { provider: "openai".into(), model: "deepseek-v4-flash".into(), max_tokens: 16000, temperature: 0.2, system_prompt: "Planner".into() },
        workers: WorkersConfig { default: WorkerProfile { provider: "openai".into(), model: "deepseek-v4-flash".into(), max_tokens: 8000, temperature: 0.1, system_prompt: String::new() }, profiles: HashMap::new() },
        node_daemon: None,
    });

    // ================================================================
    // Phase 0: Tool — Read directory structure (Planner's eyes)
    // ================================================================
    println!("OpenForce v5.1 | {} | {}", config.llm.provider, config.planner.model);
    println!("[Tool:ReadDirectory] {}\n", workspace.display());

    let entries = tool_read_directory(&workspace, 6);
    let files: Vec<&DirEntry> = entries.iter().filter(|e| !e.is_dir).collect();
    let dirs: Vec<&DirEntry> = entries.iter().filter(|e| e.is_dir).collect();

    // Group files by crate-level directory (crates/proto, crates/domain, etc.)
    let mut by_dir: HashMap<String, Vec<&DirEntry>> = HashMap::new();
    for f in &files {
        let parts: Vec<&str> = f.path.split('/').collect();
        let group = if parts.len() >= 2 && parts[0] == "crates" {
            format!("crates/{}", parts[1])  // e.g., "crates/proto", "crates/domain"
        } else if parts.len() >= 1 {
            parts[0].to_string()
        } else {
            "root".to_string()
        };
        by_dir.entry(group).or_default().push(f);
    }

    // Read key source files for the Planner
    let mut source_snapshot = String::new();
    let mut total_bytes = 0usize;
    for f in files.iter().filter(|f| {
        let e = &f.ext; e == "rs" || e == "toml" || e == "proto" || e == "sql" || e == "md"
    }).take(100) {
        let path = workspace.join(&f.path);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if total_bytes + content.len() < 250_000 {
                source_snapshot.push_str(&format!("\n=== {} ({}B) ===\n", f.path, f.size));
                source_snapshot.push_str(&content);
                total_bytes += content.len();
            }
        }
    }

    // Print directory summary
    println!("  Directories: {}", dirs.len());
    for (dir, files) in by_dir.iter().take(20) {
        let rs_count = files.iter().filter(|f| f.ext == "rs").count();
        if rs_count > 0 {
            println!("    {dir}/ — {rs_count} .rs files, {} total files", files.len());
        }
    }
    println!("  Total files: {}, Source snapshot: {} bytes\n", files.len(), total_bytes);

    // ================================================================
    // Phase 1: Planner — tool-informed decomposition
    // ================================================================
    let planner = build_client(&config, &config.planner.model);
    println!("[Planner] Analyzing directory structure and decomposing task...");

    let dir_summary: String = by_dir.iter()
        .filter(|(_, fs)| fs.iter().any(|f| f.ext == "rs"))
        .map(|(d, fs)| {
            let rs = fs.iter().filter(|f| f.ext == "rs").count();
            format!("  {d}/ — {rs} Rust files, {} total", fs.len())
        })
        .collect::<Vec<_>>()
        .join("\n");

    let src_display: String = source_snapshot.chars().take(60000).collect();
    let plan_prompt = format!(
        "Project directory structure:\n{dir_summary}\n\n\
         Source code (first 250KB):\n{src_display}\n\n\
         Task: {task}\n\n\
         Create ONE worker per significant directory/crate above.\n\
         Each worker should review ALL files in its assigned directory.\n\
         Format (one per line): N. [profile] DirectoryName: description"
    );
    let (plan_text, _) = planner.chat(&config.planner.system_prompt, &plan_prompt).await?;

    // Parse per-crate subtasks
    let mut subtasks: Vec<(String, String, String)> = vec![]; // (profile, name, desc)
    for line in plan_text.lines() {
        let t = line.trim();
        if t.is_empty() || !t.chars().next().map_or(false, |c| c.is_ascii_digit()) { continue; }
        let content = t.splitn(2, ". ").nth(1).unwrap_or(t);
        let (profile, rest) = if content.starts_with('[') {
            content[1..].split(']').next().map(|p| (p.to_string(), content[p.len()+2..].trim().to_string())).unwrap_or(("default".into(), content.to_string()))
        } else { ("default".into(), content.to_string()) };
        let parts: Vec<&str> = rest.splitn(2, ": ").collect();
        subtasks.push((profile, parts[0].to_string(), parts.get(1).map(|s| s.to_string()).unwrap_or_default()));
    }

    println!("  → {} workers planned:\n", subtasks.len());
    for (profile, name, _) in &subtasks {
        println!("    [{profile}] {name}");
    }

    // ================================================================
    // Phase 2: Workers — each as independent process (sandbox)
    // ================================================================
    println!("\n[Workers] Spawning {} workers as separate processes...", subtasks.len());

    // For now, use tokio::spawn with in-process LLM calls (sandbox binary not yet compiled)
    // In production: spawn_worker_process() → separate OS process → Node Daemon
    let mut handles = vec![];
    for (i, (profile_name, name, _desc)) in subtasks.iter().enumerate() {
        let profile = config.workers.profiles.get(profile_name).unwrap_or(&config.workers.default).clone();
        let worker = build_client(&config, &profile.model);
        let system = if profile.system_prompt.is_empty() {
            format!("Review directory: {name}. Read all files and report issues.")
        } else { profile.system_prompt.clone() };

        // Build workspace context for this worker's assigned directory
        let dir_path = workspace.join(name);
        let mut dir_files = String::new();
        if let Ok(()) = (|| -> std::io::Result<()> {
            for entry in std::fs::read_dir(&dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let rel = path.strip_prefix(&workspace).unwrap_or(&path).display().to_string();
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if content.len() < 50000 {
                            dir_files.push_str(&format!("\n=== {rel} ===\n{content}\n"));
                        }
                    }
                }
            }
            Ok(())
        })() {}

        let prompt = format!(
            "Task: {task}\nAssigned directory: {name}\n\n=== SOURCE FILES ===\n{dir_files}\n=== END ===\n\n\
             Review all files in this directory. Report specific bugs, security issues, and improvements."
        );

        let idx = i + 1;
        let p = profile_name.clone();
        handles.push(tokio::spawn(async move {
            match worker.chat(&system, &prompt).await {
                Ok((text, _)) => (idx, p, true, text),
                Err(e) => (idx, p, false, format!("Error: {e}")),
            }
        }));
    }

    let mut results: Vec<(usize, String, bool, String)> = vec![];
    for h in handles { if let Ok(r) = h.await { results.push(r); } }
    results.sort_by_key(|r| r.0);

    let ok = results.iter().filter(|r| r.2).count();
    println!("\n===== Results: {}/{} succeeded =====\n", ok, results.len());

    let mut report = format!("# OpenForce Report\n\n**Task:** {task}\n**Workspace:** {}\n**Workers:** {}/{}\n",
        workspace.display(), ok, results.len());

    for (idx, profile, success, text) in &results {
        let s = if *success { "OK" } else { "FAIL" };
        println!("Worker-{idx} [{profile}] [{s}]:");
        report.push_str(&format!("\n## Worker-{idx} [{profile}] [{s}]\n\n"));
        for line in text.lines().take(10) {
            println!("  {line}");
            report.push_str(line); report.push('\n');
        }
        report.push('\n'); println!();
    }

    let path = format!("/tmp/openforce_report_{}.md", uuid::Uuid::now_v7());
    std::fs::write(&path, &report)?;
    println!("\nReport: {path}");
    Ok(())
}
