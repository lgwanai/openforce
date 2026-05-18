use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SkillTool { pub name: String, pub description: String, pub skill_dir: String }

/// Generic SkillRunner — discovers skills via `adapter.py` protocol, invokes via subprocess.
/// No skill logic in Rust. All skill code lives in `skills/<name>/adapter.py`.
pub struct SkillRunner { tools: HashMap<String, SkillTool> }

impl SkillRunner {
    pub fn discover(skills_dir: &str) -> Self {
        let mut tools = HashMap::new();
        let dir = Path::new(skills_dir);
        if !dir.exists() { return Self { tools }; }
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let adapter = path.join("adapter.py");
            if !adapter.exists() { continue; }
            let skill_dir = path.display().to_string();
            // Progressive disclosure: discover tools from well-known skill names
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.contains("deerflow") {
                tools.insert("web_search".into(), SkillTool { name: "web_search".into(), description: "Search the web via Tavily".into(), skill_dir: skill_dir.clone() });
                tools.insert("web_fetch".into(), SkillTool { name: "web_fetch".into(), description: "Fetch web page via Jina AI".into(), skill_dir });
            }
        }
        Self { tools }
    }

    pub fn tool_summary(&self) -> String {
        if self.tools.is_empty() { return String::new(); }
        let mut s = "AVAILABLE SKILL TOOLS:\n".to_string();
        for t in self.tools.values() { s.push_str(&format!("  {} — {}\n", t.name, t.description)); }
        s.push_str("  Usage: [需网络检索: query]  or  [需使用工具: tool_name]\n");
        s
    }

    pub fn has_tool(&self, name: &str) -> bool { self.tools.contains_key(name) }

    pub async fn invoke(&self, tool_name: &str, args: &serde_json::Value) -> String {
        let tool = match self.tools.get(tool_name) { Some(t) => t, None => return format!("[工具不存在: {tool_name}]") };
        let adapter = format!("{}/adapter.py", tool.skill_dir);
        let input = serde_json::to_string(&serde_json::json!({"tool":tool_name,"args":args})).unwrap_or_default();
        match tokio::process::Command::new("python3").arg(&adapter)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        {
            Ok(mut child) => {
                use tokio::io::AsyncWriteExt;
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(input.as_bytes()).await; drop(s); }
                match child.wait_with_output().await {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        if stdout.trim().is_empty() { format!("[工具错误: {}]", String::from_utf8_lossy(&out.stderr)) } else { stdout }
                    }
                    Err(e) => format!("[执行失败: {e}]"),
                }
            }
            Err(e) => format!("[启动失败: {e}]"),
        }
    }
}
