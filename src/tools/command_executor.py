"""
Safe command executor for agent-browser and other tools.

This module provides safe execution of external commands, preventing
shell injection by using argument lists and whitelist validation.

Security guarantees:
1. Never uses shell=True
2. All commands go through CommandWhitelist
3. Environment setup done in Python, not shell
"""

import subprocess
import shutil
import os
from pathlib import Path
from typing import List, Optional

from src.security.command_whitelist import CommandWhitelist, SecurityError


# Global whitelist instance
_whitelist: Optional[CommandWhitelist] = None


def get_whitelist() -> CommandWhitelist:
    """Get or create the global command whitelist."""
    global _whitelist
    if _whitelist is None:
        _whitelist = CommandWhitelist()
    return _whitelist


def _setup_playwright_environment() -> dict:
    """
    Set up environment for playwright/agent-browser.

    Instead of using shell commands with &&, we do the setup in Python:
    1. Create cache directory if needed
    2. Set up symlinks for playwright cache
    3. Return environment dict with HOME=/tmp

    Returns:
        Environment dict for subprocess
    """
    # Target cache location (user's playwright cache)
    user_cache = Path('/Users/wuliang/Library/Caches/ms-playwright')

    # Temporary cache location (for sandboxed access)
    tmp_cache = Path('/tmp/Library/Caches/ms-playwright')

    # Create parent directory
    tmp_cache.parent.mkdir(parents=True, exist_ok=True)

    # Create symlink if target exists and link doesn't
    if user_cache.exists() and not tmp_cache.exists():
        try:
            tmp_cache.symlink_to(user_cache)
        except OSError:
            # Symlink creation failed, continue without it
            pass

    # Return environment with HOME redirected to /tmp
    env = os.environ.copy()
    env['HOME'] = '/tmp'

    return env


def run_agent_browser_safe(command_args: List[str], timeout: int = 60) -> str:
    """
    Execute agent-browser CLI command safely.

    This function replaces the vulnerable shell=True implementation with:
    1. Whitelist-based command validation
    2. Argument list execution (no shell interpretation)
    3. Python-based environment setup (no shell chaining)

    Args:
        command_args: List of arguments to pass to agent-browser
        timeout: Maximum execution time in seconds (default: 60)

    Returns:
        Combined stdout + stderr output (truncated to 5000 chars)

    Raises:
        SecurityError: If command is not in whitelist
        subprocess.TimeoutExpired: If command exceeds timeout

    Security:
        - Never uses shell=True
        - Shell injection characters in args are treated as literal text
        - Environment variables are explicitly controlled
    """
    whitelist = get_whitelist()

    # Resolve agent-browser executable
    # First try direct agent-browser command
    agent_browser_path = shutil.which('agent-browser')

    if agent_browser_path:
        # Use direct agent-browser executable
        # Ensure it's in whitelist
        if 'agent-browser' not in whitelist._allowed:
            whitelist.allow('agent-browser', agent_browser_path)
        executable_name = 'agent-browser'
    else:
        # Fallback to npx agent-browser
        # npx should already be in default whitelist
        executable_name = 'npx'
        # Prepend 'agent-browser' to args for npx
        command_args = ['agent-browser'] + command_args

    # Set up environment
    env = _setup_playwright_environment()

    try:
        result = whitelist.run(
            executable_name,
            command_args,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env
        )

        output = result.stdout
        if result.stderr:
            output += "\n" + result.stderr

        # Limit output size to prevent context overflow
        if len(output) > 5000:
            return output[:5000] + "\n... [TRUNCATED]"

        return output if output else "Command executed successfully with no output."

    except subprocess.TimeoutExpired:
        raise
    except SecurityError:
        raise
    except Exception as e:
        return f"Error executing agent-browser: {str(e)}"
