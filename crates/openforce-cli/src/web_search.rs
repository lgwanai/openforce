/// Call deerflow-skill web_search tool directly via subprocess.
pub async fn web_search(query: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let code = format!(
        "import sys; sys.path.insert(0,'{home}/workspace/deerflow-skill');\
         from deerflow.community.tavily.tools import web_search_tool;\
         import asyncio;\
         r=asyncio.run(web_search_tool.ainvoke({{'query':'{q}','max_results':3}}));\
         print(r)",
        q = query.replace('\'', "\\'").replace('"', "\\\"")
    );
    match tokio::process::Command::new("python3").arg("-c").arg(&code).output().await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if !stdout.is_empty() { stdout }
            else if !stderr.is_empty() { format!("[deerflow错误: {stderr}]") }
            else { format!("[deerflow无输出: {query}]") }
        }
        Err(e) => format!("[deerflow调用失败: {e}]"),
    }
}

#[allow(dead_code)]
pub async fn web_fetch(url: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let code = format!(
        "import sys; sys.path.insert(0,'{home}/workspace/deerflow-skill');\
         from deerflow.community.jina_ai.tools import web_fetch_tool;\
         import asyncio;\
         r=asyncio.run(web_fetch_tool.ainvoke({{'url':'{u}'}}));\
         print(r)",
        u = url
    );
    match tokio::process::Command::new("python3").arg("-c").arg(&code).output().await {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(e) => format!("[deerflow抓取失败: {e}]"),
    }
}
