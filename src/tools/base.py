import os
import time
import datetime
import platform
import json
from pathlib import Path
from typing import List, Dict, Any

# Sandbox root directory
SANDBOX_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../sandbox"))
os.makedirs(SANDBOX_ROOT, exist_ok=True)

class SecurityError(Exception):
    pass

def _resolve_and_check_path(filepath: str) -> str:
    """
    Hard constraint anti-escape: realpath normalization, check against sandbox root.
    Symlinks are not followed out of the sandbox.
    """
    if os.path.isabs(filepath):
        if filepath.startswith(SANDBOX_ROOT):
            target_path = filepath
        else:
            # Treat absolute paths as relative to sandbox root
            # lstrip('/') removes leading slashes so os.path.join works correctly
            target_path = os.path.join(SANDBOX_ROOT, filepath.lstrip('/'))
    else:
        target_path = os.path.join(SANDBOX_ROOT, filepath)
        
    real_path = os.path.realpath(target_path)
    
    if not real_path.startswith(SANDBOX_ROOT):
        raise SecurityError(f"Path escape detected: {filepath}")
        
    # Check symlinks in the path components
    current = real_path
    while current != SANDBOX_ROOT and current != "/":
        if os.path.islink(current):
            raise SecurityError(f"Symlinks are forbidden: {current}")
        current = os.path.dirname(current)
        
    return real_path

# 3.4.1 Basic Env Tools
def get_current_time() -> str:
    """获取当前标准时间、时区及星期"""
    now = datetime.datetime.now().astimezone()
    weekdays = ["星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日"]
    weekday_str = weekdays[now.weekday()]
    return f"{now.isoformat()} ({weekday_str})"

def get_system_info() -> str:
    """获取当前操作系统类型、CPU架构等"""
    return json.dumps({
        "os": platform.system(),
        "release": platform.release(),
        "architecture": platform.machine(),
        "python_version": platform.python_version()
    })

# 3.4.2 File & Path Tools
def read_file(filepath: str) -> str:
    """读取指定文件内容"""
    real_path = _resolve_and_check_path(filepath)
    if not os.path.exists(real_path):
        return f"Error: File not found {filepath}"
    if not os.path.isfile(real_path):
        return f"Error: Not a file {filepath}"
    
    with open(real_path, "r", encoding="utf-8") as f:
        return f.read()

def write_file(filepath: str, content: str) -> str:
    """覆盖写入文件"""
    real_path = _resolve_and_check_path(filepath)
    os.makedirs(os.path.dirname(real_path), exist_ok=True)
    with open(real_path, "w", encoding="utf-8") as f:
        f.write(content)
    return f"Successfully wrote to {filepath}"

def list_directory(path: str = ".") -> str:
    """列出目录下的文件和子目录"""
    real_path = _resolve_and_check_path(path)
    if not os.path.exists(real_path):
        return f"Error: Directory not found {path}"
    if not os.path.isdir(real_path):
        return f"Error: Not a directory {path}"
    
    items = os.listdir(real_path)
    return json.dumps(items)

def get_current_path() -> str:
    """获取当前工作目录 (CWD), relative to sandbox"""
    return SANDBOX_ROOT

# 3.4.4 Information & Research Tools (Mocked for now)
def web_search(query: str) -> str:
    """搜索引擎入口"""
    # Dummy implementation, can be integrated with Tavily or DuckDuckGo
    return f"Search results for: {query}"

def fetch_webpage(url: str) -> str:
    """Fetch and extract text content from a webpage (SSRF-protected).

    This function now uses SSRF protection to prevent access to internal
    network resources like localhost, private IPs, and AWS metadata endpoints.

    Args:
        url: The URL to fetch

    Returns:
        The extracted text content, or an error message if blocked or failed

    Security:
        - Blocks private IP ranges (RFC 1918, loopback, link-local)
        - Blocks non-HTTP schemes (file://, ftp://, etc.)
        - Blocks localhost hostname variants
        - Does not follow redirects
    """
    from ..security.ssrf import fetch_webpage_safe
    return fetch_webpage_safe(url, timeout=10.0, max_length=5000)

def run_baidu_search_skill(query: str) -> str:
    """使用百度千帆 AI 搜索 API 进行 Web 搜索"""
    import subprocess
    import json
    import os
    
    script_path = "/Users/wuliang/skills/baidu-search-openclaw/scripts/search.py"
    if not os.path.exists(script_path):
        return "Error: Baidu search skill script not found at " + script_path
        
    try:
        req_body = json.dumps({"query": query})
        result = subprocess.run(
            ["python3", script_path, req_body], 
            capture_output=True, 
            text=True, 
            timeout=30
        )
        output = result.stdout
        if result.stderr:
            output += "\n" + result.stderr
            
        if len(output) > 5000:
            return output[:5000] + "\n... [TRUNCATED]"
        return output
    except Exception as e:
        return f"Error executing baidu search skill: {str(e)}"

def run_agent_browser(command: str) -> str:
    """Execute agent-browser CLI command for advanced browser automation.

    This function now uses the safe command executor to prevent shell injection.
    The command string is parsed into arguments using shlex.split().

    Args:
        command: Command string to pass to agent-browser (e.g., "--help")

    Returns:
        Output from agent-browser execution

    Security:
        - Uses whitelist-based command execution
        - Never uses shell=True
        - Shell injection characters are treated as literal text
    """
    import shlex
    from .command_executor import run_agent_browser_safe

    # Parse command string into args safely
    # shlex.split handles quoting correctly
    command_args = shlex.split(command) if command else []
    return run_agent_browser_safe(command_args)

# 3.4.5 Content Compression
def summarize_content(text: str, max_length: int = 1000) -> str:
    """调用专门的轻量级模型对超长文本进行要点提取"""
    # Simple truncation for now. In real implementation, call LLM
    if len(text) <= max_length:
        return text
    return text[:max_length] + "... [TRUNCATED]"

