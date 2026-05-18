"""Subagent delegation configuration and error handling.

This module provides configuration for deer-flow subagent delegation:
- Timeout configuration with clear default (900s)
- Concurrency limit configuration (MAX_CONCURRENT_SUBAGENTS)
- Timeout error formatting with agent identification

Environment variables:
- DEER_FLOW_SUBAGENT_TIMEOUT: Subagent timeout in seconds (default: 900)
- MAX_CONCURRENT_SUBAGENTS: Maximum parallel subagents (default: 3)
"""
import os
import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from deerflow.client import DeerFlowClient

DEFAULT_SUBAGENT_TIMEOUT = 900  # 15 minutes
DEFAULT_MAX_CONCURRENT_SUBAGENTS = 3

SUBAGENT_TIMEOUT_ERRORS = {
    "subagent_timeout": """A subagent timed out after {timeout}s.

    The subagent '{agent_name}' was working on:
    {task_description}

    What to try:
    - Increase DEER_FLOW_SUBAGENT_TIMEOUT environment variable:
      export DEER_FLOW_SUBAGENT_TIMEOUT=1800
    - Simplify the subtask or break it into smaller pieces
    - Use --pro mode for sequential planning instead of parallel execution

    Current timeout: {timeout}s (15 min default)
    """,
}


def get_subagent_config() -> dict:
    """Get subagent configuration from environment variables.

    Reads:
    - DEER_FLOW_SUBAGENT_TIMEOUT: Timeout in seconds (default: 900)
    - MAX_CONCURRENT_SUBAGENTS: Max parallel subagents (default: 3)

    Returns:
        Dict with subagent_timeout and max_concurrent_subagents.
    """
    return {
        "subagent_timeout": int(os.getenv("DEER_FLOW_SUBAGENT_TIMEOUT", str(DEFAULT_SUBAGENT_TIMEOUT))),
        "max_concurrent_subagents": get_max_concurrent_subagents(),
    }


def get_max_concurrent_subagents() -> int:
    """Get maximum concurrent subagents limit (SUBA-04).

    Reads MAX_CONCURRENT_SUBAGENTS from environment.
    Falls back to DEFAULT_MAX_CONCURRENT_SUBAGENTS on error.

    Returns:
        Maximum number of subagents that can run in parallel.
    """
    default = DEFAULT_MAX_CONCURRENT_SUBAGENTS
    env_value = os.getenv("MAX_CONCURRENT_SUBAGENTS")

    if env_value is None:
        return default

    try:
        value = int(env_value)
        # Validate reasonable range
        if value < 1:
            return default
        return value
    except ValueError:
        return default


def log_subagent_config() -> None:
    """Log subagent configuration at startup.

    Prints configuration to stderr for user visibility.
    Matches logging pattern from lib/tools.py.
    """
    import sys

    max_concurrent = get_max_concurrent_subagents()
    timeout = int(os.getenv("DEER_FLOW_SUBAGENT_TIMEOUT", str(DEFAULT_SUBAGENT_TIMEOUT)))

    print("\n[Subagent Configuration]", file=sys.stderr, flush=True)
    print(f"  - Max concurrent: {max_concurrent}", file=sys.stderr, flush=True)
    print(f"  - Timeout: {timeout}s", file=sys.stderr, flush=True)


def format_subagent_timeout_error(e: Exception, timeout: int) -> str:
    """Format subagent timeout with agent identification (SUBA-03).

    Extracts agent name and task from exception message if available.
    deerflow-harness may embed context in timeout exceptions.

    Args:
        e: The timeout exception.
        timeout: Configured timeout in seconds.

    Returns:
        User-friendly error message with subagent context.
    """
    error_msg = str(e).lower()

    # Attempt to extract subagent context from error message
    # Patterns deerflow-harness may use:
    # - "Subagent 'agent_name' timed out"
    # - "subagent: agent_name timed out"
    # - "task_tool call to agent_name exceeded timeout"
    agent_name = "unknown"
    task_description = "a delegated task"

    # Pattern 1: "Subagent 'name'" or "subagent: name"
    agent_match = re.search(
        r"subagent[:\s]+['\"]?(\w+)['\"]?",
        error_msg,
        re.IGNORECASE
    )
    if agent_match:
        agent_name = agent_match.group(1)

    # Pattern 2: "task: 'description'" or "working on: description"
    task_match = re.search(
        r"(?:task|working on)[:\s]+['\"]?(.+?)['\"]?(?:\s|$)",
        error_msg,
        re.IGNORECASE
    )
    if task_match:
        task_description = task_match.group(1).strip()

    # Also check for asyncio.TimeoutError pattern
    if isinstance(e, TimeoutError) or "timeout" in error_msg:
        # Generic timeout - may not have agent context
        # deerflow-harness middleware should add this
        pass

    return SUBAGENT_TIMEOUT_ERRORS["subagent_timeout"].format(
        timeout=timeout,
        agent_name=agent_name,
        task_description=task_description,
    )


def is_subagent_timeout(e: Exception) -> bool:
    """Check if exception is a subagent timeout error.

    Args:
        e: The exception to check.

    Returns:
        True if this is a subagent-related timeout.
    """
    error_type = type(e).__name__
    error_msg = str(e).lower()

    # Check for subagent-specific timeout indicators
    # Match both "timeout" and "timed out" patterns
    if "subagent" in error_msg and ("timeout" in error_msg or "timed out" in error_msg):
        return True

    # Check for TimeoutError in subagent context
    # (detected via event stream or error type)
    if error_type in ("TimeoutError", "asyncio.TimeoutError"):
        # Could be subagent timeout - check for context
        return "subagent" in error_msg or "task_tool" in error_msg

    return False