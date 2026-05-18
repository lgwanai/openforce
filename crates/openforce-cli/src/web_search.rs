/// Load API key from local skills/deerflow/config.yaml or env var.
fn load_key(name: &str, env_var: &str) -> String {
    if let Ok(v) = std::env::var(env_var) { if !v.is_empty() { return v; } }
    let yaml = std::fs::read_to_string("skills/deerflow/config.yaml").unwrap_or_default();
    let lines: Vec<&str> = yaml.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&format!("name: {name}")) {
            for j in i..lines.len() {
                if let Some(v) = lines[j].split("api_key:").nth(1) { return v.trim().to_string(); }
            }
        }
    }
    String::new()
}

/// Web search via Tavily API. Keys from deerflow config, no Python dependency.
pub async fn web_search(query: &str) -> String {
    let api_key = load_key("web_search", "TAVILY_API_KEY");
    if api_key.is_empty() { return format!("[搜索需要 TAVILY_API_KEY] {query}"); }

    let client = reqwest::Client::new();
    match client.post("https://api.tavily.com/search")
        .json(&serde_json::json!({"api_key": api_key, "query": query, "max_results": 3, "search_depth": "basic"}))
        .timeout(std::time::Duration::from_secs(10)).send().await
    {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(body) => {
                let items: Vec<String> = body["results"].as_array().map(|a| a.iter().enumerate()
                    .map(|(i, r)| format!("{}. {} ({})\n   {}", i+1,
                        r["title"].as_str().unwrap_or(""),
                        r["url"].as_str().unwrap_or(""),
                        r["content"].as_str().unwrap_or("").chars().take(300).collect::<String>()))
                    .collect()).unwrap_or_default();
                if items.is_empty() { format!("[无结果] {query}") } else { items.join("\n\n") }
            }
            Err(e) => format!("[解析失败: {e}]"),
        },
        Err(e) => format!("[搜索失败: {e}]"),
    }
}

#[allow(dead_code)]
pub async fn web_fetch(url: &str) -> String {
    let api_key = load_key("web_fetch", "JINA_API_KEY");
    if api_key.is_empty() { return format!("[抓取需要 JINA_API_KEY] {url}"); }
    let client = reqwest::Client::new();
    match client.get(format!("https://r.jina.ai/{url}"))
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_secs(15)).send().await
    {
        Ok(r) => r.text().await.map(|t| t.chars().take(4000).collect()).unwrap_or_else(|e| format!("[读取失败: {e}]")),
        Err(e) => format!("[抓取失败: {e}]"),
    }
}
