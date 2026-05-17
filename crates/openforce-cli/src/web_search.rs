use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TavilyResult { title: String, url: String, content: String }
#[derive(Debug, Deserialize)]
struct TavilyResponse { results: Vec<TavilyResult> }

/// Search the web using Tavily API (keys from deerflow-skill config).
pub async fn web_search(query: &str) -> String {
    let api_key = std::env::var("TAVILY_API_KEY").unwrap_or_else(|_| load_deerflow_key("web_search"));
    if api_key.is_empty() { return format!("[搜索不可用: TAVILY_API_KEY] 建议手动提供关于 '{query}' 的信息"); }

    let client = reqwest::Client::new();
    let resp = match client.post("https://api.tavily.com/search")
        .json(&serde_json::json!({"api_key": api_key, "query": query, "max_results": 3, "search_depth": "basic"}))
        .timeout(std::time::Duration::from_secs(10)).send().await
    {
        Ok(r) => r, Err(e) => return format!("[搜索失败: {e}]"),
    };
    let body: TavilyResponse = match resp.json().await {
        Ok(b) => b, Err(e) => return format!("[解析失败: {e}]"),
    };
    if body.results.is_empty() { return format!("[无结果: {query}]"); }
    body.results.iter().enumerate()
        .map(|(i, r)| format!("{}. {} ({})\n   {}", i+1, r.title, r.url, r.content.chars().take(300).collect::<String>()))
        .collect::<Vec<_>>().join("\n\n")
}

/// Fetch web page via Jina AI Reader.
pub async fn web_fetch(url: &str) -> String {
    let api_key = std::env::var("JINA_API_KEY").unwrap_or_else(|_| load_deerflow_key("web_fetch"));
    if api_key.is_empty() { return format!("[抓取不可用: JINA_API_KEY] URL: {url}"); }
    let client = reqwest::Client::new();
    let resp = match client.get(format!("https://r.jina.ai/{url}"))
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_secs(15)).send().await
    {
        Ok(r) => r, Err(e) => return format!("[抓取失败: {e}]"),
    };
    resp.text().await.map(|t| t.chars().take(4000).collect()).unwrap_or_else(|e| format!("[读取失败: {e}]"))
}

// ── deerflow-skill config loading ──

#[derive(Debug, Deserialize)]
struct DeerFlowCfg { tools: Vec<DeerFlowTool> }
#[derive(Debug, Deserialize)]
struct DeerFlowTool { name: String, api_key: Option<String> }

fn load_deerflow_key(tool_name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::Path::new(&home).join("workspace/deerflow-skill/config.yaml");
    let yaml = std::fs::read_to_string(&path).ok().unwrap_or_default();
    serde_yaml::from_str::<DeerFlowCfg>(&yaml).ok()
        .and_then(|c| c.tools.iter().find(|t| t.name == tool_name).and_then(|t| t.api_key.clone()))
        .unwrap_or_default()
}
