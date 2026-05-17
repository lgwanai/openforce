/// Call deerflow-skill directly as external process. No code duplication.
#[allow(dead_code)]
pub async fn web_search(query: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let skill_dir = format!("{home}/workspace/deerflow-skill");
    match tokio::process::Command::new("python3")
        .arg("-c")
        .arg(format!(
            "import sys; sys.path.insert(0,'{dir}'); \
             from deerflow.community.tavily.tools import web_search_tool; \
             import asyncio; \
             result = asyncio.run(web_search_tool.ainvoke({{'query': '{q}', 'max_results': 3}})); \
             print(result)",
            dir = skill_dir, q = query.replace('\'', "\\'")
        ))
        .output().await
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                format!("[搜索无结果或deerflow未安装: {query}]")
            } else {
                stdout.trim().to_string()
            }
        }
        Err(e) => format!("[deerflow调用失败: {e}]"),
    }
}

pub async fn web_fetch(url: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let skill_dir = format!("{home}/workspace/deerflow-skill");
    match tokio::process::Command::new("python3")
        .arg("-c")
        .arg(format!(
            "import sys; sys.path.insert(0,'{dir}'); \
             from deerflow.community.jina_ai.tools import web_fetch_tool; \
             import asyncio; \
             result = asyncio.run(web_fetch_tool.ainvoke({{'url': '{u}'}})); \
             print(result)",
            dir = skill_dir, u = url
        ))
        .output().await
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(e) => format!("[deerflow抓取失败: {e}]"),
    }
}
