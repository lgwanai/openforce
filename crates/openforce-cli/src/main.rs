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
mod dag_executor;
mod agent_registry;
use session_manager::SessionManager;
use repl::SessionRepl;
use agent_registry::AgentRegistry;

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

fn build_client(config: &Config, provider: &str, model: &str) -> LlmClient {
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    let api_base = config.llm.api_base.clone();
    match provider {
        "anthropic" => LlmClient::anthropic(api_key, api_base).with_model(model),
        _ => LlmClient::openai(api_key, api_base.unwrap_or("https://api.openai.com/v1".into()), model.to_string())
    }
}

/// Scan for [DONE:n] markers in worker output text.
/// Returns Some(step_number) if found.
fn scan_done_marker(line: &str) -> Option<usize> {
    let t = line.trim();
    if let Some(idx) = t.find("[DONE:") {
        let rest = &t[idx + 6..];
        if let Some(end) = rest.find(']') {
            return rest[..end].parse::<usize>().ok();
        }
    }
    // Also match [done:n] (lowercase) and [Done:n]
    if let Some(idx) = t.to_lowercase().find("[done:") {
        let rest = &t[idx + 6..];
        if let Some(end) = rest.find(']') {
            return rest[..end].parse::<usize>().ok();
        }
    }
    None
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

    // ── Slash Command: /skill:name <task> or /name <task> ──
    // Semantically classifies the task FIRST, then injects skill as available tool.
    // Skill body is loaded on-demand by Worker (progressive disclosure Level 2).
    if subcmd.starts_with('/') {
        let raw = subcmd.trim_start_matches('/');
        let (skill_name, user_task): (String, String) = {
            let mut parts = raw.splitn(2, |c: char| c == ' ' || c == ':');
            let name = parts.next().unwrap_or(raw).to_string();
            // Also collect remaining args (args[2..]) as part of the task
            let from_split = parts.next().unwrap_or("").trim().to_string();
            let from_args = args.get(2..).map(|a| a.join(" ")).unwrap_or_default();
            let task = if from_args.is_empty() { from_split } else { from_args };
            (name, task)
        };
        let workspace = resolve_workspace(&args);
        let skills_dir = workspace.join("skills");
        let registry = openforce_skill::SkillRegistry::discover_with_config(
            skills_dir.to_str().unwrap_or("skills"), None,
        );

        // Resolve skill name (exact or fuzzy)
        let resolved_name: Option<String> = if registry.load_skill_body(&skill_name).is_some() {
            Some(skill_name.clone())
        } else {
            let names = registry.enabled_skill_names();
            let matches: Vec<&String> = names.iter()
                .filter(|n| n.contains(&skill_name) || n.to_lowercase().contains(&skill_name.to_lowercase()))
                .collect();
            if matches.len() == 1 {
                Some(matches[0].clone())
            } else if matches.is_empty() {
                eprintln!("Skill '{skill_name}' not found. Available: {}", names.join(", "));
                std::process::exit(1);
            } else {
                eprintln!("Multiple matches for '{skill_name}': {:?}. Be more specific.", matches);
                std::process::exit(1);
            }
        };

        if let Some(ref name) = resolved_name {
            // Task goes through semantic classification + Planner normally.
            // The skill is injected as metadata — Worker loads body on-demand.
            let task = if user_task.is_empty() {
                format!("Execute the '{}' skill workflow on the current project", name)
            } else {
                user_task.clone()
            };
            println!("[/{name}] skill bound — task will be semantically classified first");
            let mut mgr = SessionManager::new(workspace.clone()).await;
            let mut session = mgr.create(&task).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            session.bound_skill = Some(name.clone());
            session.save().map_err(|e| anyhow::anyhow!("{e}"))?;
            return run_pipeline(workspace, task, Some(session)).await;
        }
    }

    match subcmd {
        "skills" => {
            let ws = resolve_workspace(&args);
            let skills_dir = ws.join("skills");
            let registry = openforce_skill::SkillRegistry::discover_with_config(
                skills_dir.to_str().unwrap_or("skills"), None,
            );
            let names = registry.enabled_skill_names();
            println!("Available skills ({} total):", names.len());
            for name in &names {
                if let Some(body) = registry.load_skill_body(name) {
                    let first_line = body.lines().next().unwrap_or("");
                    let desc = if let Some(skill) = registry.get(name) {
                        skill.frontmatter.description.chars().take(100).collect::<String>()
                    } else { first_line.chars().take(80).collect::<String>() };
                    println!("  /{name} — {desc}");
                } else {
                    println!("  /{name}");
                }
            }
            return Ok(());
        }
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
            // Load and show todo progress
            let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
            if !redis_url.is_empty() {
                if let Ok(sid_uuid) = uuid::Uuid::parse_str(&session.session_id.to_string()) {
                    if let Ok(mut store) = openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                        if let Ok(Some(tl)) = store.get_todo_list(&sid_uuid).await {
                            println!("[Todo] {}", tl.progress_str());
                            for item in &tl.items {
                                let icon = match item.status.as_str() {
                                    "completed" => "✓", "in_progress" => "▶", "failed" => "✗", _ => "○"
                                };
                                println!("  [{icon}] #{:02} {}", item.id, item.content.chars().take(80).collect::<String>());
                            }
                        }
                    }
                }
            }
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
            let _ws = resolve_workspace(&args);
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
    eprintln!("  openforce skills                             List available skills");
    eprintln!("  openforce sessions                           List active sessions");
    eprintln!("  openforce cancel <session-id>                Cancel a session");
    eprintln!("  openforce terminate <session-id> <worker-id>  Terminate a worker");
    eprintln!("  openforce /skill:name [<task>]               Activate a skill by name");
    eprintln!("  openforce /name [<task>]                     Shorthand for skill activation");
    eprintln!("  openforce /<partial-name> [<task>]           Fuzzy match skill name");
}

async fn run_pipeline(workspace: PathBuf, task: String, session: Option<session_state::LocalSessionState>) -> Result<()> {
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    if api_key.len() < 5 { return Err(anyhow::anyhow!("API_KEY not set")); }
    let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
    let session_id = session.as_ref().map(|s| s.session_id.to_string());

    let config: Config = toml::from_str(&std::fs::read_to_string("openforce.toml").unwrap_or_default())
        .unwrap_or_else(|_| Config {
            llm: LlmConfig { provider: "openai".into(), api_base: Some("https://api.deepseek.com".into()) },
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
    let planner = build_client(&config, &config.planner.provider, &config.planner.model);
    println!("[Planner] Semantic classification...");

    let kb = KnowledgeBase::load("experts").unwrap_or_else(|_| KnowledgeBase {
        index: openforce_knowledge_base::ExpertIndex { version: "1.0".into(), categories: HashMap::new(), model_tiers: HashMap::new() },
        profiles: HashMap::new(), base_dir: String::new(),
    });

    // Load Agent Registry (~184 agent roles from agency-agents)
    let agents_path = workspace.join("crates/openforce-cli/agents");
    let agent_registry = AgentRegistry::load(&if agents_path.exists() { agents_path } else { PathBuf::from("crates/openforce-cli/agents") })
        .unwrap_or_else(|e| { eprintln!("  [AgentRegistry] {e} — continuing without"); AgentRegistry::empty() });
    if !agent_registry.is_empty() {
        println!("[AgentRegistry] {} agents in {} domains", agent_registry.len(), agent_registry.domains().len());
    }

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

    // Build bound skill hint for Planner context
    let bound_skill_hint = if let Some(ref bs) = session.as_ref().and_then(|s| s.bound_skill.as_ref()) {
        format!(
            "<bound_skill name=\"{bs}\">The user activated this skill via /{bs}. Use skill_load to get its full instructions when relevant to the task.</bound_skill>\n\n"
        )
    } else { String::new() };

    // Build agent registry metadata for Planner context (progressive disclosure Level 1)
    let agent_catalog = if !agent_registry.is_empty() {
        agent_registry.metadata_for_planner()
    } else { String::new() };

    // ── Planner RoundTable: MECE-validated task decomposition ──
    println!("[Planner] RoundTable...");
    let mut subtasks: Vec<(String, String, String, Vec<String>, Vec<String>)> = vec![]; // (role, title, desc, dependencies, files)

    let skills_dir_path = workspace.join("skills");
    let skills_dir_str = skills_dir_path.to_str().unwrap_or("skills").to_string();
    match planner_roundtable::run_roundtable(&planner, &task, &classification, &available_roles,
        &agent_catalog,
        &skills_dir_str,
        &format!("{bound_skill_hint}--- Project Structure ---\n{dir_summary}")).await {
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
                subtasks.push((t.role.clone(), t.title.clone(), desc, t.dependencies.clone(), t.files.clone()));
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
                                subtasks.push((role, desc, String::new(), vec![], vec![]));
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
            subtasks.push((role.clone(), format!("{role}: {desc}"), String::new(), vec![], vec![]));
        }
    }

    let mut ri = 0usize;
    println!("\n  Workers:");
    for (pf, name, _, _, _) in &subtasks {
        let ef = if pf == "default" && ri < classification.suggested_roles.len() {
            let r = classification.suggested_roles[ri].clone(); ri += 1; r
        } else if pf == "default" && ri < matched_profiles.len() {
            let r = matched_profiles[ri].name.clone(); ri += 1; r
        } else { pf.clone() };
        println!("    [{ef}] {name}");
    }
    println!();

    // ── Create Session Todo List in Redis ──
    let mut todo_list: Option<openforce_redis_session::store::SessionTodoList> = None;
    if !redis_url.is_empty() {
        if let Some(ref sid) = session_id {
            if let Ok(sid_uuid) = uuid::Uuid::parse_str(sid) {
                match openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                    Ok(mut store) => {
                        // Try to load existing todo list (for resume)
                        let existing = store.get_todo_list(&sid_uuid).await.unwrap_or(None);
                        if let Some(ref existing_list) = existing {
                            println!("[Todo] Loaded existing: {}", existing_list.progress_str());
                            todo_list = existing;
                        } else {
                            // Build fresh todo list from subtasks
                            let todos: Vec<(String, String, String, Vec<String>)> = subtasks.iter()
                                .map(|(role, title, desc, deps, _files)| (role.clone(), title.clone(), desc.clone(), deps.clone()))
                                .collect();
                            let tl = openforce_redis_session::store::SessionTodoList::from_task_tree(sid, &todos);
                            if let Err(e) = store.set_todo_list(&sid_uuid, &tl).await {
                                eprintln!("  [Todo] save failed: {e}");
                            } else {
                                println!("[Todo] {} tasks persisted to Redis", tl.total);
                            }
                            todo_list = Some(tl);
                        }
                    }
                    Err(e) => eprintln!("  [Todo] Redis connect failed: {e}"),
                }
            }
        }
    }

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

    // Phase 3: Workers — DAG wave execution (Scheduler-style)
    let worker_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("target/debug/openforce"))
        .parent().map(|p| p.join("worker")).unwrap_or_else(|| PathBuf::from("target/debug/worker"));
    let use_process = worker_bin.exists();
    if use_process { println!("[Workers] DAG waves (独立进程): {worker_bin:?}"); }
    else { println!("[Workers] In-process fallback"); }

    // Build DAG and compute execution waves
    let dag_tasks = dag_executor::build_dag(&subtasks, &Default::default(), None).unwrap_or_else(|e| {
        eprintln!("[DAG] build_dag failed: {e:?} — empty DAG");
        vec![]
    });
    let plan = dag_executor::compute_waves(&dag_tasks, &Default::default(), None).unwrap_or_else(|e| {
        eprintln!("[DAG] compute_waves failed: {e:?} — falling back to single wave");
        dag_executor::ExecutionPlan { waves: vec![(0..dag_tasks.len()).collect()], max_concurrent: 1 }
    });
    let waves = &plan.waves;
    if waves.len() > 1 { println!("[DAG] {} waves (max concurrent: {}): {}", waves.len(), plan.max_concurrent, waves.iter().map(|w| w.len().to_string()).collect::<Vec<_>>().join(" → ")); }

    // Collect all role names from subtasks and resolve against agent registry
    let planner_roles: Vec<&str> = subtasks.iter().map(|(pn, _, _, _, _)| pn.as_str()).collect();
    let (found_agents, missing_roles) = agent_registry.resolve_exact(
        &planner_roles.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    if !found_agents.is_empty() {
        println!("[Agents] {} roles resolved: {}",
            found_agents.len(),
            found_agents.iter().map(|a| format!("{}{}", a.emoji.as_deref().unwrap_or(""), a.name)).collect::<Vec<_>>().join(", ")
        );
    }
    for m in &missing_roles {
        println!("  ⚠ role '{}' not in agent registry — using default config", m);
    }

    let bound_skill_for_worker = session.as_ref().and_then(|s| s.bound_skill.clone());

    let max_concurrent = plan.max_concurrent;

    let mut results: Vec<(usize,String,bool,String,String)> = vec![];
    let mut failed_titles: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (wave_num, wave) in waves.iter().enumerate() {
        if waves.len() > 1 { println!("\n[Wave {}/{}] {} workers (max concurrent: {max_concurrent})", wave_num+1, waves.len(), wave.len()); }
        let mut wave_handles = vec![];

        // Chunk wave into batches of max_concurrent to avoid resource exhaustion
        for chunk in wave.chunks(max_concurrent) {
        for &task_idx in chunk {
            let bound_skill_for_task = bound_skill_for_worker.clone();
            let redis_url_c = redis_url.clone();
            // ── Skip if upstream dependency failed ──
            let deps_blocked = dag_tasks.get(task_idx).map(|dt| dt.depends_on.iter().any(|d| failed_titles.contains(d.as_str()))).unwrap_or(false);
            if deps_blocked {
                let (pn, name, _, _, _) = &subtasks[task_idx];
                println!("  [DAG] task-{} '[{}] {}' BLOCKED — upstream failed", task_idx+1, pn, name);
                results.push((task_idx+1, pn.clone(), false, "blocked".into(), "Upstream dependency failed".into()));
                failed_titles.insert(name.clone()); // cascade: downstream-of-blocked also blocked
                continue;
            }
            // ── Resume: skip already-completed tasks ──
            if let Some(ref tl) = todo_list {
                if let Some(item) = tl.items.get(task_idx) {
                    if item.is_done() {
                        println!("  [Todo] task-{} '{}' already done — skipped", item.id, item.content.chars().take(60).collect::<String>());
                        results.push((task_idx + 1, item.agent.clone(), true, "completed".into(), format!("(resumed) {}", item.content)));
                        continue;
                    }
                }
            }

            let (pn, name, _desc, _deps, rt_files) = &subtasks[task_idx];
            let i = task_idx;
            // Resolve agent system prompt: registry > knowledge base > config profiles > default
            let (provider, model, sp) = if let Some(ap) = agent_registry.get(pn) {
                // Agent registry: use full agent personality as system prompt
                (config.workers.default.provider.clone(),
                 config.workers.default.model.clone(),
                 ap.system_prompt.clone())
            } else if let Some(kp) = kb.profiles.get(pn) {
                ("openai".to_string(), kp.default_model.clone(), kp.system_prompt.clone())
            } else if let Some(cp) = config.workers.profiles.get(pn) {
                (cp.provider.clone(), cp.model.clone(), cp.system_prompt.clone())
            } else { (config.workers.default.provider.clone(), config.workers.default.model.clone(), config.workers.default.system_prompt.clone()) };
            let idx = i+1; let pnc = pn.clone(); let provider_c = provider.clone();
        let task_c = task.clone(); let name_c = name.clone();
        let workspace_c = workspace.clone();
        let api_key_c = api_key.clone();
        let base_url_c = config.llm.api_base.clone().unwrap_or_else(|| "https://api.deepseek.com".into());
        let wb = worker_bin.clone();
        let tools_addr = std::env::var("PROJECT_TOOLS_ADDR").unwrap_or_else(|_| "127.0.0.1:50053".into());
        let session_id_c = session_id.clone();

        // Use RoundTable's file list if provided, validate existence, fallback to matching
        let review_paths: Vec<String> = if !rt_files.is_empty() {
            let all_files: Vec<String> = by_dir.iter()
                .flat_map(|(_, ents)| ents.iter().map(|e| workspace_c.join(&e.path).display().to_string()))
                .collect();
            rt_files.iter().map(|f| {
                let p = std::path::Path::new(f);
                let resolved = if p.is_absolute() { f.clone() } else { workspace_c.join(f).display().to_string() };
                // If file exists, use it directly
                if std::path::Path::new(&resolved).exists() {
                    return resolved;
                }
                // Try matching by filename in available files
                let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or(f);
                if let Some(matched) = all_files.iter().find(|af| af.ends_with(file_name)) {
                    return matched.clone();
                }
                // Try matching by parent dir
                let parent = p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).unwrap_or("");
                if !parent.is_empty() {
                    if let Some(matched) = all_files.iter().find(|af| af.contains(parent)) {
                        return matched.clone();
                    }
                }
                // Last resort: return original (worker will try its own fallback)
                resolved
            }).collect()
        } else {
            // Fallback: keyword matching
            let task_lower = task.to_lowercase();
            let name_lower = name.to_lowercase();
            let mut paths: Vec<String> = by_dir.iter()
                .filter(|(k,_)| {
                    let k_lower = k.to_lowercase();
                    name_lower.contains(&k_lower) || k_lower.split('/').any(|seg| name_lower.contains(seg))
                        || task_lower.contains(&k_lower)
                })
                .flat_map(|(_, ents)| { let w = workspace_c.clone(); ents.iter().map(move |e| w.join(&e.path).display().to_string()) })
                .take(50)
                .collect();
            if paths.is_empty() {
                paths = by_dir.iter()
                    .flat_map(|(_, ents)| { let w = workspace_c.clone(); ents.iter().map(move |e| w.join(&e.path).display().to_string()) })
                    .take(50)
                    .collect();
            }
            paths
        };
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
            wave_handles.push(tokio::spawn(async move {
                let task_json = serde_json::json!({
                    "task": task_c, "subtask": name_c, "profile_name": pnc,
                    "provider": provider_c, "model": model, "system_prompt": sp,
                    "api_key": api_key_c, "base_url": base_url_c,
                    "output_file": format!("/tmp/worker_{idx}_output.json"),
                    "review_paths": review_paths_c,
                    "project_tools_addr": tools_addr,
                    "session_id": session_id_c,
                    "session_map": session_map_c,
                    "redis_url": redis_url_c,
                    "skills_dir": workspace_c.join("skills").to_str().unwrap_or("skills").to_string(),
                    "bound_skill": bound_skill_for_task,
                });
                let task_file = format!("/tmp/openforce_worker_{idx}.json");
                let _ = std::fs::write(&task_file, serde_json::to_string(&task_json).unwrap_or_default());
                let start = std::time::Instant::now();
                match tokio::process::Command::new(&wb).arg("--task-file").arg(&task_file)
                    .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
                {
                    Ok(mut child) => match child.wait().await {
                        Ok(_) => {
                            let elapsed = start.elapsed().as_secs();
                            let out_file = format!("/tmp/worker_{idx}_output.json");
                            if let Ok(data) = std::fs::read_to_string(&out_file) {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                    let success = v["success"].as_bool().unwrap_or(false);
                                    let action = v["action"].as_str().unwrap_or("").to_string();
                                    let text = match action.as_str() {
                                        "stalled" => format!("STALLED: {}", v["detail"].as_str().unwrap_or("no progress")),
                                        "timeout" => format!("TIMEOUT: {}", v["detail"].as_str().unwrap_or("exceeded limit")),
                                        "error" => format!("ERROR: {}", v["reason"].as_str().unwrap_or("unknown")),
                                        _ => v["output"].as_str().unwrap_or("").to_string(),
                                    };
                                    return (idx, v["profile"].as_str().unwrap_or(&pnc).to_string(), success, action, format!("[{elapsed}s] {text}"));
                                }
                            }
                            (idx, pnc, false, "error".into(), format!("[{elapsed}s] Worker-{idx} no output"))
                        }
                        Err(e) => (idx, pnc, false, "error".into(), format!("Worker-{idx} wait error: {e}")),
                    },
                    Err(e) => (idx, pnc, false, "error".into(), format!("Worker-{idx} spawn error: {e}")),
                }
            }));
        } else {
            // Fallback: in-process LLM call (same tool-based approach with session context)
            let worker = build_client(&config, &provider, &model);
            let system = if sp.is_empty() { format!("Worker: {name}") } else { sp };
            let paths_list = review_paths.iter().map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n");
            let session_block = if !session_map.is_empty() { format!("\n\nSESSION CONTEXT:\n{session_map}") } else { String::new() };
            let prompt = format!("Task: {task}\nSubtask: {name}\n\nFiles to review:\n{paths_list}{session_block}\n\nUse read_file() tool to access source files. Produce detailed review with specific file references.");
            wave_handles.push(tokio::spawn(async move {
                match worker.chat(&system, &prompt).await {
                    Ok((text,_)) => (idx, pnc, true, "completed".into(), text),
                    Err(e) => (idx, pnc, false, "error".into(), format!("Error: {e}")),
                }
            }));
        }
    } // end for task_idx in wave

        // Wait for all workers in this chunk to complete (parallel within chunk)
        let chunk_results: Vec<(usize,String,bool,String,String)> = futures::future::join_all(wave_handles).await
            .into_iter().filter_map(|r| r.ok()).collect();
        // Track failed task titles so downstream tasks get blocked
        for (ri, _, success, _, _) in &chunk_results {
            if !success {
                if let Some((_, title, _, _, _)) = subtasks.get(ri.saturating_sub(1)) {
                    failed_titles.insert(title.clone());
                }
            }
        }
        results.extend(chunk_results);
        wave_handles = vec![]; // reset for next chunk
        } // end for chunk in wave.chunks
    } // end for wave in waves

    // Sort results by original task index for consistent display
    results.sort_by_key(|r| r.0);

    // ── Update Todo & Persist worker snapshots to Redis ──
    let mut todo_updated = false;
    if let Some(ref mut tl) = todo_list {
        for (idx, _pf, success, _action, _text) in &results {
            if *success {
                tl.mark_done(*idx, None);  // idx is already 1-based (task_idx+1)
            } else {
                tl.mark_failed(*idx);
            }
        }
        todo_updated = true;
    }

    // ── Persist worker snapshots to Redis Session ──
    if !redis_url.is_empty() {
        if let Some(ref sid) = session_id {
            if let Ok(sid_uuid) = uuid::Uuid::parse_str(sid) {
                match openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                    Ok(mut store) => {
                        let caller = openforce_redis_session::store::Caller::Scheduler { session_id: sid_uuid };
                        for (idx, pf, success, _, text) in &results {
                            let (memory_subs, _memory_findings) = {
                                let of = format!("/tmp/worker_{idx}_output.json");
                                std::fs::read_to_string(&of).ok()
                                    .and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
                                    .and_then(|v| {
                                        let ms = &v["memory_snapshot"];
                                        let st: Vec<openforce_redis_session::store::WorkerSubTask> = ms["subtasks"].as_array()
                                            .map(|a| a.iter().filter_map(|t| Some(openforce_redis_session::store::WorkerSubTask {
                                                id: t["id"].as_u64().unwrap_or(0) as usize,
                                                description: t["description"].as_str().unwrap_or("").into(),
                                                status: t["status"].as_str().unwrap_or("pending").into(),
                                                output: t["output"].as_str().unwrap_or("").into(),
                                            })).collect()).unwrap_or_default();
                                        Some((st, vec![] as Vec<String>))
                                    }).unwrap_or_default()
                            };
                            let snapshot = openforce_redis_session::store::WorkerSnapshot {
                                worker_id: format!("worker-{idx}"),
                                role: pf.clone(),
                                task: subtasks.get(*idx).map(|(_,t,_,_,_)| t.clone()).unwrap_or_default(),
                                subtasks: memory_subs,
                                acceptance_criteria: vec![],
                                progress: if *success { "completed".into() } else { "failed".into() },
                                inputs: vec![],
                                outputs: vec![text.clone()],
                                intermediate_artifacts: vec![],
                                created_at: chrono::Utc::now(),
                                updated_at: chrono::Utc::now(),
                            };
                            if let Err(e) = store.set_worker_snapshot(&caller, &snapshot).await {
                                eprintln!("  [redis] snapshot save failed for worker-{idx}: {e}");
                            } else {
                                eprintln!("  [redis] worker-{idx} snapshot saved");
                            }
                        }
                        // Persist phase result
                        let phase_summary = results.iter().map(|(i, pf, ok, action, text)| {
                            format!("Worker-{} [{}] {}: {}", i, pf, if *ok {"OK"} else {action}, text)
                        }).collect::<Vec<_>>().join(" | ");
                        let phase_result = openforce_redis_session::store::PhaseResultEntry {
                            phase: session.as_ref().map(|s| s.current_phase.as_str().to_string()).unwrap_or_default(),
                            tasks_total: results.len(),
                            tasks_ok: results.iter().filter(|r| r.2).count(),
                            plan_epoch: session.as_ref().map(|s| s.plan_epoch).unwrap_or(1),
                            summary: phase_summary,
                            timestamp: chrono::Utc::now(),
                        };
                        let _ = store.add_result(&sid_uuid, &phase_result).await;
                        // Save updated todo list
                        if let Some(ref tl) = todo_list {
                            if todo_updated {
                                let _ = store.set_todo_list(&sid_uuid, tl).await;
                                println!("  [Todo] {} — saved to Redis", tl.progress_str());
                            }
                        }
                        eprintln!("  [redis] session {sid} phase results saved");
                    }
                    Err(e) => eprintln!("  [redis] connect failed: {e}"),
                }
            }
        }
    }

    let stalled_count = results.iter().filter(|r| r.3 == "stalled" || r.3 == "timeout" || r.3 == "error").count();

    let ok = results.iter().filter(|r| r.2).count();
    println!("\n===== Results: {}/{} =====\n", ok, results.len());

    // ── [DONE:n] Detection ──
    let mut done_markers: Vec<(String, usize)> = vec![];
    for (idx, _, _, _, text) in &results {
        for line in text.lines() {
            if let Some(n) = scan_done_marker(line) {
                done_markers.push((format!("Worker-{idx}"), n));
            }
        }
    }
    if !done_markers.is_empty() {
        println!("  [DONE markers]: {}", done_markers.iter().map(|(w,n)| format!("{w}→#{n}")).collect::<Vec<_>>().join(", "));
    }

    // ── Progress visualization ──
    if let Some(ref tl) = todo_list {
        println!("  [Progress] {}", tl.progress_str());
        for item in &tl.items {
            let icon = match item.status.as_str() {
                "completed" => "✓", "in_progress" => "▶", "failed" => "✗", _ => "○"
            };
            println!("    [{icon}] #{:02} [{}] {}", item.id, item.priority, item.content.chars().take(80).collect::<String>());
        }
    }

    let mut report = format!("# OpenForce Report\n\nTask: {task}\nRoles: {:?}\nWorkers: {}/{}\n",
        classification.suggested_roles, ok, results.len());

    for (idx, pf, success, action, text) in &results {
        let s = if *success { "OK" } else { "FAIL" };
        println!("Worker-{idx} [{pf}] [{s}]:");
        report.push_str(&format!("\n## Worker-{idx} [{pf}]\n"));
        for line in text.lines() { println!("  {line}"); report.push_str(line); report.push('\n'); }
        report.push('\n'); println!();
    }

    // ── Re-plan if too many workers stalled ──
    if stalled_count > 0 && stalled_count * 2 > results.len() {
        println!("
[!] Scheduler: {}/{} workers stalled/failed", stalled_count, results.len());
        let failed_summary: String = results.iter()
            .filter(|r| r.3 == "stalled" || r.3 == "timeout" || r.3 == "error")
            .map(|(i, pf, _, action, text)| {
                format!("Worker-{i} [{pf}] {action}: {}", text.to_string())
            })
            .collect::<Vec<_>>().join(" | ");
        println!("  Reason: {failed_summary}");
        report.push_str(&format!("\n## Re-plan Required\n{}/{} workers stalled: {}\n",
            stalled_count, results.len(), failed_summary));
        report.push_str("\nRun: openforce continue <session-id> to re-plan and re-execute\n");
    }

    // Auto-record experience
    if ok > 0 {
        let _ = std::fs::create_dir_all("experts/experience");
        let exp = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(), "task": task,
            "categories": classification.categories,
            "roles": classification.suggested_roles,
            "workers": ok, "total": results.len(),
            "decomposition": results.iter().map(|(i,p,s,_a,t)| serde_json::json!({
                "id":i,"profile":p,"success":s,"summary":t.chars().take(500).collect::<String>()
            })).collect::<Vec<_>>()
        });
        let ep = format!("experts/experience/session_{}.json", uuid::Uuid::now_v7().to_string().chars().take(8).collect::<String>());
        let _ = std::fs::write(&ep, serde_json::to_string_pretty(&exp).unwrap_or_default());
    }

    // Persist planner decomposition to Redis
    if let Some(ref sess) = session {
        if !redis_url.is_empty() {
            if let Ok(mut store) = openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                let _ = store.set_planner(&sess.session_id,
                    &format!("{:?}", classification.categories),
                    &subtasks.iter().map(|(r,n,_,_,_)| format!("[{r}] {n}")).collect::<Vec<_>>().join("; "),
                    &classification.suggested_roles,
                ).await;
            }
        }
    }

    let report_prefix = session.as_ref().map(|s| s.session_id.to_string().chars().take(8).collect::<String>()).unwrap_or_else(|| "latest".into());

    // Persist session state for multi-turn support
    if let Some(mut sess) = session {
        sess.add_phase_result(session_state::PhaseResult {
            phase: sess.current_phase.clone(),
            tasks_total: results.len(),
            tasks_ok: ok,
            worker_outputs: results.iter().map(|(i, pf, success, _action, text)| session_state::WorkerOutput {
                worker_id: format!("worker-{i}"),
                role: pf.clone(),
                status: if *success { "ok".into() } else { "failed".into() },
                output: text.to_string(),
            }).collect(),
            plan_epoch: sess.plan_epoch,
        });
        sess.last_summary = Some(format!("{}/{} workers completed", ok, results.len()));

        // Check if we're at a gate-requiring phase
        let next = sess.current_phase.next_phase();
        if let Some(n) = next {
            if n.is_gate() {
                sess.advance_phase(n.clone());
                let gate = openforce_domain::session_phase::ConfirmationGate::new(
                    sess.session_id, n.clone(),
                    format!("Phase {} completed: {}/{} tasks OK", sess.current_phase.as_str(), ok, results.len()),
                    sess.plan_epoch,
                );
                sess.set_gate(&gate);
                println!("\n[Gate: {}] {} tasks completed.", n.as_str(), ok);
                println!("  Review the results above, then:");
                println!("    openforce approve   — to continue");
                println!("    openforce reject \"<feedback>\" — to modify");
            } else {
                sess.advance_phase(n.clone());
                println!("\n[Phase → {}] Auto-advancing. Continue with: openforce continue", n.as_str());
            }
        }
        sess.save().map_err(|e| anyhow::anyhow!("session save: {e}"))?;
    }

    let rp = format!("/tmp/openforce_report_{report_prefix}.md");
    std::fs::write(&rp, &report)?;
    println!("

╔══════════════════════════════════════╗");
    println!("║  Report: {}", rp);
    println!("╚══════════════════════════════════════╝
");
    Ok(())
}
