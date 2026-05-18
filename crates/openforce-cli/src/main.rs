use anyhow::Result;
use openforce_llm_client::LlmClient;
use openforce_knowledge_base::{KnowledgeBase, semantic_classify};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

mod session_state;
mod session_manager;
mod gate_handler;
mod repl;
mod planner_roundtable;
mod skill_runner;
use session_manager::SessionManager;
use repl::SessionRepl;

#[derive(Debug, Deserialize)]
struct Config { llm: LlmConfig, planner: PlannerConfig, workers: WorkersConfig }
#[derive(Debug, Deserialize)]
struct LlmConfig { provider: String, api_base: Option<String> }
#[derive(Debug, Deserialize)]
struct PlannerConfig { provider: String, model: String, max_tokens: u32, temperature: f64, system_prompt: String }
#[derive(Debug, Deserialize)]
struct WorkersConfig { default: WorkerProfile, profiles: HashMap<String, WorkerProfile> }
#[derive(Debug, Clone, Deserialize)]
struct WorkerProfile { provider: String, model: String, max_tokens: u32, temperature: f64, #[serde(default)] system_prompt: String }

#[derive(Debug, Clone)]
struct DirEntry { path: String, is_dir: bool, size: u64, ext: String }

fn build_client(config: &Config, model: &str) -> LlmClient {
    let key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    match config.llm.provider.as_str() {
        "anthropic" => LlmClient::anthropic(std::env::var("ANTHROPIC_API_KEY").unwrap_or(key)).with_model(model),
        _ => LlmClient::openai(key, config.llm.api_base.clone().unwrap_or("https://api.openai.com/v1".into()), model.to_string())
    }
}

fn truncate_at_char(s: &str, max_chars: usize) -> String {
    s.char_indices().nth(max_chars).map(|(i, _)| s[..i].to_string()).unwrap_or_else(|| s.to_string())
}

fn read_directory(root: &PathBuf, max_depth: usize) -> Vec<DirEntry> {
    let mut entries = vec![];
    for e in walkdir::WalkDir::new(root).max_depth(max_depth).into_iter().filter_map(|r| r.ok())
        .filter(|e| !e.path().to_string_lossy().contains("/target/"))
        .filter(|e| !e.path().to_string_lossy().contains("/.git/")).take(2000)
    {
        let path = e.path();
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let size = if path.is_file() { std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) } else { 0 };
        entries.push(DirEntry { path: rel, is_dir: path.is_dir(), size, ext });
    }
    entries
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 { print_usage(); std::process::exit(1); }

    // Subcommand routing for multi-turn session support
    let subcmd = args[1].as_str();
    match subcmd {
        "sessions" => {
            let ws = resolve_workspace(&args);
            return SessionManager::new(ws).await.list().await.map_err(|e| anyhow::anyhow!("{e}"));
        }
        "continue" => {
            let ws = resolve_workspace(&args);
            let sid = args.get(2).map(|s| s.as_str());
            let mut mgr = SessionManager::new(ws.clone()).await;
            let session = mgr.resume(sid).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            let interactive = args.iter().any(|a| a == "--interactive" || a == "-i");
            let phase_str = session.current_phase.as_str().to_string();
            let goal = session.goal.clone();
            println!("Resumed session — phase: {phase_str}");
            if interactive {
                let mut repl = SessionRepl::new(session, ws, true);
                return repl.run().await.map_err(|e| anyhow::anyhow!("{e}"));
            }
            return run_pipeline(ws, goal, Some(session)).await;
        }
        "approve" => {
            let ws = resolve_workspace(&args);
            let sid = args.get(2).map(|s| s.as_str());
            let mut mgr = SessionManager::new(ws.clone()).await;
            let session = mgr.approve(sid).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            let goal = session.goal.clone();
            println!("Continuing from {} phase...", session.current_phase.as_str());
            return run_pipeline(ws, goal, Some(session)).await;
        }
        "reject" => {
            let ws = resolve_workspace(&args);
            let feedback = args.get(2).cloned().unwrap_or_default();
            let sid = args.get(3).map(|s| s.as_str());
            let mut mgr = SessionManager::new(ws.clone()).await;
            let session = mgr.reject(sid, &feedback).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            let goal = format!("{}. User feedback: {feedback}", session.goal);
            return run_pipeline(ws, goal, Some(session)).await;
        }
        "cancel" => {
            if args.len() < 3 { eprintln!("Usage: openforce cancel <session-id>"); std::process::exit(1); }
            let ws = resolve_workspace(&args);
            return SessionManager::new(ws).await.cancel(&args[2]).await.map_err(|e| anyhow::anyhow!("{e}"));
        }
        "terminate" => {
            if args.len() < 4 { eprintln!("Usage: openforce terminate <session-id> <worker-id>"); std::process::exit(1); }
            let ws = resolve_workspace(&args);
            let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
            if redis_url.is_empty() { return Err(anyhow::anyhow!("REDIS_URL required for terminate")); }
            let sid = uuid::Uuid::parse_str(&args[2]).map_err(|e| anyhow::anyhow!("invalid session id: {e}"))?;
            let wid = &args[3];
            let mut store = openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await
                .map_err(|e| anyhow::anyhow!("redis: {e}"))?;
            let caller = openforce_redis_session::store::Caller::Scheduler { session_id: sid };
            store.terminate_worker(&caller, wid).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Worker {wid} terminated in session {sid}");
            return Ok(());
        }
        _ => {} // fall through to "new" (default pipeline)
    }

    // Default: "openforce new <task>" or legacy "openforce <task>"
    let (workspace, task) = if (args[1] == "new" || args[1] == "--workspace" || args[1] == "-w") {
        parse_new_args(&args)
    } else {
        (std::env::current_dir().unwrap_or_default(), args[1..].join(" "))
    };

    let mut mgr = SessionManager::new(workspace.clone()).await;
    let session = mgr.create(&task).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    run_pipeline(workspace, task, Some(session)).await
}

fn resolve_workspace(args: &[String]) -> PathBuf {
    for i in 1..args.len() {
        if (args[i] == "--workspace" || args[i] == "-w") && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    std::env::current_dir().unwrap_or_default()
}

fn parse_new_args(args: &[String]) -> (PathBuf, String) {
    let mut ws = std::env::current_dir().unwrap_or_default();
    let mut task = String::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "new" => { i += 1; continue; }
            "--workspace" | "-w" => { if i + 1 < args.len() { ws = PathBuf::from(&args[i + 1]); i += 2; } else { i += 1; } }
            _ => { task = args[i..].join(" "); break; }
        }
    }
    if task.is_empty() { eprintln!("Usage: openforce new <task> [--workspace <path>]"); std::process::exit(1); }
    (ws, task)
}

fn print_usage() {
    eprintln!("OpenForce v5.2 — Multi-turn Session CLI");
    eprintln!("  openforce new <task> [--workspace <path>]   Create a new session");
    eprintln!("  openforce continue [<session-id>]            Resume latest/specific session");
    eprintln!("  openforce approve [<session-id>]             Approve pending gate");
    eprintln!("  openforce reject \"<feedback>\" [<session-id>]  Reject with feedback");
    eprintln!("  openforce sessions                           List active sessions");
    eprintln!("  openforce cancel <session-id>                Cancel a session");
    eprintln!("  openforce terminate <session-id> <worker-id>  Terminate a worker (Scheduler/Planner only)");
}

async fn run_pipeline(workspace: PathBuf, task: String, session: Option<session_state::LocalSessionState>) -> Result<()> {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    if api_key.len() < 5 { return Err(anyhow::anyhow!("OPENAI_API_KEY not set")); }
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into());

    let config: Config = toml::from_str(&std::fs::read_to_string("openforce.toml").unwrap_or_default())
        .unwrap_or_else(|_| Config {
            llm: LlmConfig { provider: "openai".into(), api_base: Some(base_url.clone()) },
            planner: PlannerConfig { provider: "openai".into(), model: "deepseek-v4-flash".into(), max_tokens: 16000, temperature: 0.2, system_prompt: "Planner".into() },
            workers: WorkersConfig { default: WorkerProfile { provider: "openai".into(), model: "deepseek-v4-flash".into(), max_tokens: 8000, temperature: 0.1, system_prompt: String::new() }, profiles: HashMap::new() },
        });

    // Phase 0: Tool — Read directory
    println!("OpenForce v5.1 | {} | {}", config.llm.provider, config.planner.model);
    println!("[ReadDirectory] {}\n", workspace.display());
    let entries = read_directory(&workspace, 6);
    let files: Vec<&DirEntry> = entries.iter().filter(|e| !e.is_dir).collect();

    let mut by_dir: HashMap<String, Vec<&DirEntry>> = HashMap::new();
    for f in &files {
        let parts: Vec<&str> = f.path.split('/').collect();
        let group = if parts.len() >= 2 && parts[0] == "crates" { format!("crates/{}", parts[1]) }
            else if parts.len() >= 1 { parts[0].to_string() } else { "root".to_string() };
        by_dir.entry(group).or_default().push(f);
    }

    let mut source_snapshot = String::new();
    let mut total = 0usize;
    // Read ALL source files (model has 1M context — no need to truncate early)
    for f in files.iter().filter(|f| matches!(f.ext.as_str(), "rs"|"toml"|"proto"|"sql"|"md")) {
        if let Ok(c) = std::fs::read_to_string(workspace.join(&f.path)) {
            if total + c.len() < 500_000 { source_snapshot.push_str(&format!("\n=== {} ===\n{}", f.path, c)); total += c.len(); }
        }
    }

    for (d, ents) in by_dir.iter().take(20) {
        let n = ents.iter().filter(|f| f.ext == "rs").count();
        if n > 0 { println!("  {d}/ — {n} rs, {} files", ents.len()); }
    }
    println!("  {} files, {}B source\n", files.len(), total);

    // Phase 1: Semantic classification (LLM-powered, NOT keyword matching)
    let planner = build_client(&config, &config.planner.model);
    println!("[Planner] Semantic classification...");

    let kb = KnowledgeBase::load("experts").unwrap_or_else(|_| KnowledgeBase {
        index: openforce_knowledge_base::ExpertIndex { version: "1.0".into(), categories: HashMap::new(), model_tiers: HashMap::new() },
        profiles: HashMap::new(), base_dir: String::new(),
    });

    let classification = semantic_classify(&planner, &task, &kb).await?;
    let matched_profiles = kb.get_profiles(&classification.suggested_roles);
    println!("  categories: {:?}", classification.categories);
    println!("  roles: {:?} ({} matches)", classification.suggested_roles, matched_profiles.len());
    println!("  complexity: {}", classification.complexity);

    // Build context
    let mut sop_ctx = String::new();
    for cat in &classification.categories {
        if let Some(ci) = kb.index.categories.get(cat) {
            if let Some(sn) = &ci.sop {
                if let Ok(s) = std::fs::read_to_string(format!("experts/sop/{sn}.md")) {
                    sop_ctx.push_str(&format!("\n--- SOP: {sn} ---\n{s}\n"));
                }
            }
        }
    }
    let mut profile_ctx = String::new();
    for p in &matched_profiles {
        profile_ctx.push_str(&format!("\n[{}] ({}): {}", p.name, p.default_model, p.system_prompt.chars().take(200).collect::<String>()));
    }

    let dir_summary: String = by_dir.iter()
        .filter(|(_, fs)| fs.iter().any(|f| f.ext == "rs"))
        .map(|(d, fs)| format!("  {d}/")).collect::<Vec<_>>().join("\n");
    let src_display: String = source_snapshot; // full source — model has 1M context

    let available_roles: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        classification.suggested_roles.iter()
            .chain(matched_profiles.iter().map(|p| &p.name))
            .filter(|r| seen.insert(r.to_string()))
            .cloned().collect()
    };

    // ── Planner RoundTable: MECE-validated task decomposition ──
    println!("[Planner] RoundTable: 3 agents × 3 rounds...");
    let mut subtasks: Vec<(String, String, String)> = vec![];

    match planner_roundtable::run_roundtable(&planner, &task, &classification, &available_roles, &dir_summary).await {
        Ok(tree) => {
            println!("  MECE: {}, confidence: {}", tree.mece_validated, tree.confidence);
            if !tree.needs_info.is_empty() {
                println!("  ⚠ 信息缺口 ({} 项): 运行 'openforce research' 补充", tree.needs_info.len());
            }
            if !tree.goal.specific.is_empty() {
                println!("  SMART: {}", tree.goal.specific.chars().take(100).collect::<String>());
            }
            for t in &tree.tasks {
                let desc = if t.acceptance_criteria.is_empty() {
                    t.objective.clone()
                } else {
                    format!("{} | 验收: {}", t.objective, t.acceptance_criteria.first().unwrap_or(&String::new()))
                };
                subtasks.push((t.role.clone(), t.title.clone(), desc));
            }
        }
        Err(e) => {
            println!("  RoundTable failed: {e} — falling back to direct decomposition");
            // Fallback: direct LLM decomposition
            let plan_prompt = format!(
                "Task: {task}\nRoles: [{roles}]\nDirs:\n{dir_summary}\n\n\
                 Decompose into specific subtasks. Format:\n1. [role] Title: description",
                roles = available_roles.join(", ")
            );
            if let Ok((plan_text, _)) = planner.chat(&config.planner.system_prompt, &plan_prompt).await {
                for line in plan_text.lines() {
                    let t = line.trim();
                    if t.is_empty() || !t.chars().next().map_or(false, |c| c.is_ascii_digit()) { continue; }
                    let stripped = t.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ' || c == '-' || c == '*');
                    if let Some(idx) = stripped.find(']') {
                        if stripped.starts_with('[') {
                            let role = stripped[1..idx].to_string();
                            let desc = stripped[idx+1..].trim().trim_start_matches(':').trim().to_string();
                            if !role.is_empty() && !desc.is_empty() {
                                subtasks.push((role, desc, String::new()));
                            }
                        }
                    }
                }
            }
        }
    }

    // Last resort: one task per role
    if subtasks.is_empty() && !classification.suggested_roles.is_empty() {
        for (i, role) in classification.suggested_roles.iter().enumerate() {
            let desc = if i == 0 { task.clone() } else { format!("{task} (independent verification)") };
            subtasks.push((role.clone(), format!("{role}: {desc}"), String::new()));
        }
    }

    let mut ri = 0usize;
    println!("\n  Workers:");
    for (pf, name, _) in &subtasks {
        let ef = if pf == "default" && ri < classification.suggested_roles.len() {
            let r = classification.suggested_roles[ri].clone(); ri += 1; r
        } else if pf == "default" && ri < matched_profiles.len() {
            let r = matched_profiles[ri].name.clone(); ri += 1; r
        } else { pf.clone() };
        println!("    [{ef}] {name}");
    }
    println!();

    // Phase 2: Build dir→files map
    let mut dir_files: HashMap<String, String> = HashMap::new();
    for (dn, ents) in &by_dir {
        let t: String = ents.iter().filter(|e| matches!(e.ext.as_str(), "rs"|"toml"|"proto"|"sql"|"md")).take(25)
            .map(|e| {
                let c = std::fs::read_to_string(workspace.join(&e.path)).unwrap_or_default();
                format!("\n=== {} ===\n{}", e.path, if c.len()>15000 {truncate_at_char(&c, 15000)+"\n..."} else {c})
            }).collect::<Vec<_>>().join("\n");
        dir_files.insert(dn.clone(), t);
    }

    // Phase 3: Workers (独立进程 or in-process fallback)
    let worker_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("target/debug/openforce"))
        .parent().map(|p| p.join("worker")).unwrap_or_else(|| PathBuf::from("target/debug/worker"));
    let use_process = worker_bin.exists();
    if use_process { println!("[Workers] Independent OS processes: {worker_bin:?}"); }
    else { println!("[Workers] In-process fallback (build worker binary for process isolation)"); }

    let mut handles = vec![];
    for (i, (pn, name, _)) in subtasks.iter().enumerate() {
        let (model, sp) = if let Some(kp) = kb.profiles.get(pn) {
            (kp.default_model.clone(), kp.system_prompt.clone())
        } else if let Some(cp) = config.workers.profiles.get(pn) {
            (cp.model.clone(), cp.system_prompt.clone())
        } else { (config.workers.default.model.clone(), config.workers.default.system_prompt.clone()) };
        let idx = i+1; let pnc = pn.clone();
        let task_c = task.clone(); let name_c = name.clone();
        let workspace_c = workspace.clone();
        let api_key_c = api_key.clone();
        let base_url_c = base_url.clone();
        let wb = worker_bin.clone();
        let tools_addr = std::env::var("PROJECT_TOOLS_ADDR").unwrap_or_else(|_| "127.0.0.1:50053".into());
        let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
        let session_id = session.as_ref().map(|s| s.session_id.to_string());
        let session_id_c = session_id.clone();

        // Assign specific files/directories by matching task description keywords against crate names
        let task_lower = task.to_lowercase();
        let name_lower = name.to_lowercase();
        let review_paths: Vec<String> = by_dir.iter()
            .filter(|(k,_)| {
                let k_lower = k.to_lowercase();
                name_lower.contains(&k_lower) || k_lower.split('/').any(|seg| name_lower.contains(seg))
                    || task_lower.contains(&k_lower)
            })
            .flat_map(|(d, ents)| { let w = workspace_c.clone(); ents.iter().map(move |e| w.join(&e.path).display().to_string()) })
            .take(50)
            .collect();
        // If still no matches, use all source files up to limit
        let review_paths: Vec<String> = if review_paths.is_empty() {
            by_dir.iter()
                .flat_map(|(_, ents)| { let w = workspace_c.clone(); ents.iter().map(move |e| w.join(&e.path).display().to_string()) })
                .take(50)
                .collect()
        } else { review_paths };
        let review_paths_c = review_paths.clone();

        // Build session map for worker context
        let session_map = if let Some(ref sid) = session_id_c {
            if !redis_url.is_empty() {
                match openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                    Ok(mut store) => store.get_session_map(
                        &uuid::Uuid::parse_str(sid).unwrap_or(uuid::Uuid::nil()),
                        &format!("worker-{idx}")
                    ).await.unwrap_or_default(),
                    Err(_) => String::new(),
                }
            } else { String::new() }
        } else { String::new() };
        let session_map_c = session_map.clone();

        if use_process {
            // Spawn as independent OS process
            handles.push(tokio::spawn(async move {
                let task_json = serde_json::json!({
                    "task": task_c, "subtask": name_c, "profile_name": pnc,
                    "model": model, "system_prompt": sp,
                    "api_key": api_key_c, "base_url": base_url_c,
                    "output_file": format!("/tmp/worker_{idx}_output.json"),
                    "review_paths": review_paths_c,
                    "project_tools_addr": tools_addr,
                    "session_id": session_id_c,
                    "session_map": session_map_c,
                    "redis_url": redis_url,
                });
                let task_file = format!("/tmp/openforce_worker_{idx}.json");
                let _ = std::fs::write(&task_file, serde_json::to_string(&task_json).unwrap_or_default());
                match std::process::Command::new(&wb).arg("--task-file").arg(&task_file)
                    .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
                {
                    Ok(mut child) => match child.wait() {
                        Ok(_) => {
                            let out_file = format!("/tmp/worker_{idx}_output.json");
                            if let Ok(data) = std::fs::read_to_string(&out_file) {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                    let success = v["success"].as_bool().unwrap_or(false);
                                    let action = v["action"].as_str().unwrap_or("");
                                    let text = match action {
                                        "reject" => format!("REJECTED: {}", v["reason"].as_str().unwrap_or("unknown")),
                                        "exhausted" => format!("EXHAUSTED: {}", v["reason"].as_str().unwrap_or("max cycles")),
                                        _ => v["output"].as_str().unwrap_or("").to_string(),
                                    };
                                    let cycles = v["cycles"].as_u64().unwrap_or(1);
                                    let label = format!("{} ({} cycle{})", action, cycles, if cycles > 1 {"s"} else {""});
                                    return (idx, v["profile"].as_str().unwrap_or(&pnc).to_string(), success, text);
                                }
                            }
                            (idx, pnc, false, format!("Worker-{idx} no output"))
                        }
                        Err(e) => (idx, pnc, false, format!("Worker-{idx} wait error: {e}")),
                    },
                    Err(e) => (idx, pnc, false, format!("Worker-{idx} spawn error: {e}")),
                }
            }));
        } else {
            // Fallback: in-process LLM call (same tool-based approach with session context)
            let worker = build_client(&config, &model);
            let system = if sp.is_empty() { format!("Worker: {name}") } else { sp };
            let paths_list = review_paths.iter().map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n");
            let session_block = if !session_map.is_empty() { format!("\n\nSESSION CONTEXT:\n{session_map}") } else { String::new() };
            let prompt = format!("Task: {task}\nSubtask: {name}\n\nFiles to review:\n{paths_list}{session_block}\n\nUse read_file() tool to access source files. Produce detailed review with specific file references.");
            handles.push(tokio::spawn(async move {
                match worker.chat(&system, &prompt).await {
                    Ok((text,_)) => (idx, pnc, true, text),
                    Err(e) => (idx, pnc, false, format!("Error: {e}")),
                }
            }));
        }
    }

    let mut results: Vec<(usize,String,bool,String)> = vec![];
    for h in handles { if let Ok(r) = h.await { results.push(r); } }
    results.sort_by_key(|r| r.0);

    let ok = results.iter().filter(|r| r.2).count();
    println!("\n===== Results: {}/{} =====\n", ok, results.len());

    let mut report = format!("# OpenForce Report\n\nTask: {task}\nRoles: {:?}\nWorkers: {}/{}\n",
        classification.suggested_roles, ok, results.len());

    for (idx, pf, success, text) in &results {
        let s = if *success { "OK" } else { "FAIL" };
        println!("Worker-{idx} [{pf}] [{s}]:");
        report.push_str(&format!("\n## Worker-{idx} [{pf}]\n"));
        for line in text.lines().take(10) { println!("  {line}"); report.push_str(line); report.push('\n'); }
        report.push('\n'); println!();
    }

    // Auto-record experience
    if ok > 0 {
        let _ = std::fs::create_dir_all("experts/experience");
        let exp = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(), "task": task,
            "categories": classification.categories,
            "roles": classification.suggested_roles,
            "workers": ok, "total": results.len(),
            "decomposition": results.iter().map(|(i,p,s,t)| serde_json::json!({
                "id":i,"profile":p,"success":s,"summary":t.chars().take(300).collect::<String>()
            })).collect::<Vec<_>>()
        });
        let ep = format!("experts/experience/session_{}.json", uuid::Uuid::now_v7().to_string().chars().take(8).collect::<String>());
        let _ = std::fs::write(&ep, serde_json::to_string_pretty(&exp).unwrap_or_default());
    }

    // Persist to Redis (planner + worker snapshots) if available
    if let Some(ref sess) = session {
        let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
        if !redis_url.is_empty() {
            if let Ok(mut store) = openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                // Save planner decomposition
                let _ = store.set_planner(&sess.session_id,
                    &format!("{:?}", classification.categories),
                    &subtasks.iter().map(|(r,n,_)| format!("[{r}] {n}")).collect::<Vec<_>>().join("; "),
                    &classification.suggested_roles,
                ).await;
                // Save worker snapshots
                for (i, pf, success, text) in &results {
                    let wid = format!("worker-{i}");
                    let caller = openforce_redis_session::store::Caller::Worker {
                        worker_id: wid.clone(), session_id: sess.session_id,
                    };
                    let _ = store.set_worker_snapshot(&caller,
                        &openforce_redis_session::store::WorkerSnapshot {
                            worker_id: wid,
                            role: pf.clone(), task: task.clone(),
                            subtasks: vec![],
                            acceptance_criteria: vec![],
                            progress: if *success { "done" } else { "failed" }.into(),
                            inputs: vec![], outputs: vec![text.chars().take(500).collect()],
                            intermediate_artifacts: vec![],
                            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
                        },
                    ).await;
                }
            }
        }
    }

    // Persist session state for multi-turn support
    if let Some(mut sess) = session {
        sess.add_phase_result(session_state::PhaseResult {
            phase: sess.current_phase,
            tasks_total: results.len(),
            tasks_ok: ok,
            worker_outputs: results.iter().map(|(i, pf, success, text)| session_state::WorkerOutput {
                worker_id: format!("worker-{i}"),
                role: pf.clone(),
                status: if *success { "ok".into() } else { "failed".into() },
                output: text.chars().take(500).collect(),
            }).collect(),
            plan_epoch: sess.plan_epoch,
        });
        sess.last_summary = Some(format!("{}/{} workers completed", ok, results.len()));

        // Check if we're at a gate-requiring phase
        let next = sess.current_phase.next_phase();
        if let Some(n) = next {
            if n.is_gate() {
                sess.advance_phase(n);
                let gate = openforce_domain::session_phase::ConfirmationGate::new(
                    sess.session_id, n,
                    format!("Phase {} completed: {}/{} tasks OK", sess.current_phase.as_str(), ok, results.len()),
                    sess.plan_epoch,
                );
                sess.set_gate(&gate);
                println!("\n[Gate: {}] {} tasks completed.", n.as_str(), ok);
                println!("  Review the results above, then:");
                println!("    openforce approve   — to continue");
                println!("    openforce reject \"<feedback>\" — to modify");
            } else {
                sess.advance_phase(n);
                println!("\n[Phase → {}] Auto-advancing. Continue with: openforce continue", n.as_str());
            }
        }
        sess.save().map_err(|e| anyhow::anyhow!("session save: {e}"))?;
    }

    let rp = format!("/tmp/openforce_report_{}.md", uuid::Uuid::now_v7());
    std::fs::write(&rp, &report)?;
    println!("Report: {rp}");
    Ok(())
}
