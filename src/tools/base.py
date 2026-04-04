import os
import time
import datetime
import platform
import json
from pathlib import Path
from typing import List, Dict, Any, Optional, Callable, Union
from enum import Enum

from ..security.taint_engine import (
    TaintedValue,
    TaintSource,
    TaintEngine,
    taint_source,
)

# Project root directory (where the agent is running)
PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../"))
os.makedirs(os.path.join(PROJECT_ROOT, "sandbox"), exist_ok=True)

# Sandbox root directory for untrusted operations
SANDBOX_ROOT = os.path.join(PROJECT_ROOT, "sandbox")


class AccessLevel(Enum):
    """Access level for file operations."""
    SANDBOX_ONLY = "sandbox_only"      # Only sandbox directory, no approval needed
    PROJECT_READ = "project_read"      # Read project files, needs approval
    PROJECT_WRITE = "project_write"    # Write project files, needs explicit approval


class SecurityError(Exception):
    pass


class ApprovalRequest(Exception):
    """Raised when an operation needs user approval."""
    def __init__(self, message: str, operation: str, path: str, details: dict = None):
        self.message = message
        self.operation = operation
        self.path = path
        self.details = details or {}
        super().__init__(message)


# Approval callback - set by CLI or other interface
_approval_callback: Optional[Callable[[str, str, str, dict], bool]] = None


def set_approval_callback(callback: Callable[[str, str, str, dict], bool]):
    """Set the approval callback function."""
    global _approval_callback
    _approval_callback = callback


def _request_approval(operation: str, path: str, details: dict = None) -> bool:
    """Request user approval for an operation."""
    if _approval_callback is None:
        # No callback set, default to deny for safety
        raise SecurityError(f"Operation '{operation}' on '{path}' requires approval but no approval callback is set")

    return _approval_callback(operation, path, details or {})


def _resolve_and_check_path(filepath: str, access_level: AccessLevel = AccessLevel.SANDBOX_ONLY) -> str:
    """
    Resolve path and check access permissions.

    Args:
        filepath: The path to resolve
        access_level: Required access level

    Returns:
        The resolved absolute path

    Raises:
        SecurityError: If path escape detected or symlink forbidden
        ApprovalRequest: If operation needs user approval
    """
    # Determine the base root based on access level
    if access_level == AccessLevel.SANDBOX_ONLY:
        base_root = SANDBOX_ROOT
    else:
        base_root = PROJECT_ROOT

    # Resolve path
    if os.path.isabs(filepath):
        if filepath.startswith(base_root):
            target_path = filepath
        elif access_level != AccessLevel.SANDBOX_ONLY and filepath.startswith(PROJECT_ROOT):
            # Accessing project directory with elevated permissions
            target_path = filepath
        else:
            # Treat absolute paths outside base_root as relative to base_root
            target_path = os.path.join(base_root, filepath.lstrip('/'))
    else:
        target_path = os.path.join(base_root, filepath)

    real_path = os.path.realpath(target_path)

    # Check if path is within allowed boundaries
    if access_level == AccessLevel.SANDBOX_ONLY:
        if not real_path.startswith(SANDBOX_ROOT):
            raise SecurityError(f"Path escape detected: {filepath} (resolved to {real_path})")
    else:
        if not real_path.startswith(PROJECT_ROOT):
            raise SecurityError(f"Path escape outside project: {filepath} (resolved to {real_path})")

    # Check symlinks in the path components
    current = real_path
    while current != PROJECT_ROOT and current != "/":
        if os.path.islink(current):
            raise SecurityError(f"Symlinks are forbidden: {current}")
        current = os.path.dirname(current)

    # Request approval for project-level access
    if access_level != AccessLevel.SANDBOX_ONLY and not real_path.startswith(SANDBOX_ROOT):
        operation_type = "读取" if access_level == AccessLevel.PROJECT_READ else "写入"
        if not _request_approval(operation_type, real_path, {"access_level": access_level.value}):
            raise SecurityError(f"Access denied: {filepath}")

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
        "python_version": platform.python_version(),
        "project_root": PROJECT_ROOT,
        "sandbox_root": SANDBOX_ROOT
    })


# 3.4.2 File & Path Tools
def read_file(filepath: str, sandbox_only: bool = False) -> str:
    """读取指定文件内容

    Args:
        filepath: 文件路径
        sandbox_only: 如果为 True，只允许访问 sandbox 目录

    Returns:
        文件内容
    """
    access_level = AccessLevel.SANDBOX_ONLY if sandbox_only else AccessLevel.PROJECT_READ
    try:
        real_path = _resolve_and_check_path(filepath, access_level)
    except SecurityError as e:
        return f"Error: {str(e)}"

    if not os.path.exists(real_path):
        return f"Error: File not found {filepath}"
    if not os.path.isfile(real_path):
        return f"Error: Not a file {filepath}"

    with open(real_path, "r", encoding="utf-8") as f:
        return f.read()


def write_file(filepath: str, content: Union[str, TaintedValue], sandbox_only: bool = False) -> str:
    """覆盖写入文件

    Args:
        filepath: 文件路径
        content: 文件内容 (字符串或 TaintedValue)
        sandbox_only: 如果为 True，只允许访问 sandbox 目录

    Returns:
        操作结果
    """
    access_level = AccessLevel.SANDBOX_ONLY if sandbox_only else AccessLevel.PROJECT_WRITE
    try:
        real_path = _resolve_and_check_path(filepath, access_level)
    except SecurityError as e:
        return f"Error: {str(e)}"

    # Check taint level for content
    if isinstance(content, TaintedValue):
        if not TaintEngine.check_tool_call("write_file", {"content": content.value}, {"content": content}):
            return json.dumps({"error": "Cannot write untrusted content to file. Sanitize data first."})
        content_str = content.value
    else:
        content_str = content

    os.makedirs(os.path.dirname(real_path), exist_ok=True)
    with open(real_path, "w", encoding="utf-8") as f:
        f.write(content_str)
    return f"Successfully wrote to {filepath}"


def list_directory(path: str = ".", sandbox_only: bool = False) -> List[str]:
    """列出目录下的文件和子目录

    Args:
        path: 目录路径
        sandbox_only: 如果为 True，只允许访问 sandbox 目录

    Returns:
        文件和目录列表
    """
    access_level = AccessLevel.SANDBOX_ONLY if sandbox_only else AccessLevel.PROJECT_READ
    try:
        real_path = _resolve_and_check_path(path, access_level)
    except SecurityError as e:
        return [f"Error: {str(e)}"]

    if not os.path.exists(real_path):
        return [f"Error: Directory not found {path}"]
    if not os.path.isdir(real_path):
        return [f"Error: Not a directory {path}"]

    items = os.listdir(real_path)
    return items


def get_current_path() -> str:
    """获取项目根目录"""
    return PROJECT_ROOT


def get_sandbox_path() -> str:
    """获取沙箱目录"""
    return SANDBOX_ROOT


# 3.4.4 Information & Research Tools (Mocked for now)
@taint_source(TaintSource.SEARCH)
def web_search(query: str) -> str:
    """搜索引擎入口"""
    # Dummy implementation, can be integrated with Tavily or DuckDuckGo
    return f"Search results for: {query}"


@taint_source(TaintSource.WEB)
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
        - Returns TaintedValue marked with WEB source
    """
    from ..security.ssrf import fetch_webpage_safe
    return fetch_webpage_safe(url, timeout=10.0, max_length=5000)


@taint_source(TaintSource.SEARCH)
def run_baidu_search_skill(query: str) -> str:
    """使用百度千帆 AI 搜索 API 进行 Web 搜索"""
    import subprocess
    import json
    from ..core.config import get_external_tools_config

    config = get_external_tools_config()
    script_path = config.baidu_search_script

    if not script_path:
        return json.dumps({"error": "Baidu search script not configured. Set OPENFORCE_BAIDU_SEARCH_SCRIPT environment variable."})

    if not script_path.exists():
        return json.dumps({"error": f"Baidu search script not found at {script_path}"})

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
