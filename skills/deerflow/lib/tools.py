"""Tool registry utilities for MCP and built-in tool handling.

This module provides:
- TOOL-01: Built-in tools exposure
- TOOL-02: MCP tools visibility and naming
- TOOL-03: Tool deduplication
- TOOL-04: MCP status logging
- TOOL-05: MCP availability warnings
"""
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from langchain_core.tools import BaseTool


def log_available_tools(tools: list["BaseTool"]) -> None:
    """Log available tools to stderr (TOOL-01).

    Prints tool names for visibility during initialization.

    Args:
        tools: List of tools from get_available_tools().
    """
    print(f"\n[Tools available: {len(tools)}]", file=sys.stderr, flush=True)
    for tool in tools:
        print(f"  - {tool.name}", file=sys.stderr, flush=True)


def get_unique_tool_names(tools: list["BaseTool"]) -> list[str]:
    """Get unique tool names, removing duplicates (TOOL-03).

    Args:
        tools: List of tools potentially with duplicate names.

    Returns:
        List of unique tool names, preserving first occurrence order.
    """
    seen: set[str] = set()
    names: list[str] = []
    for tool in tools:
        if tool.name not in seen:
            seen.add(tool.name)
            names.append(tool.name)
    return names


def get_mcp_tool_names(mcp_tools: list["BaseTool"]) -> list[str]:
    """Get names of MCP tools (TOOL-02).

    MCP tools are prefixed with server name: "mcp__{server}__{tool}"

    Args:
        mcp_tools: List of MCP tools from get_cached_mcp_tools().

    Returns:
        List of MCP tool names.
    """
    return [tool.name for tool in mcp_tools]


def log_mcp_status(
    servers: dict[str, dict],
    mcp_tools: list["BaseTool"]
) -> None:
    """Log MCP server connection status (TOOL-04).

    Prints to stderr for visibility:
    - Number of configured MCP servers
    - Each server name and transport type
    - Number of loaded MCP tools

    Args:
        servers: Dict of {server_name: {"type": str, "enabled": bool}}
                 from ExtensionsConfig.get_enabled_mcp_servers().
        mcp_tools: List of MCP tools from get_cached_mcp_tools().
    """
    if not servers:
        print("[No MCP servers configured]", file=sys.stderr, flush=True)
        return

    print(f"\n[MCP servers configured: {len(servers)}]", file=sys.stderr, flush=True)
    for name, config in servers.items():
        transport = config.get("type", "stdio")
        status = "enabled" if config.get("enabled", True) else "disabled"
        print(f"  - {name} ({transport}): {status}", file=sys.stderr, flush=True)

    print(f"[MCP tools loaded: {len(mcp_tools)}]", file=sys.stderr, flush=True)
    for tool in mcp_tools:
        print(f"  - {tool.name}", file=sys.stderr, flush=True)


def check_mcp_tool_availability(
    servers: dict[str, dict],
    mcp_tools: list["BaseTool"]
) -> list[str]:
    """Check for expected but unavailable MCP tools (TOOL-05).

    Warns when an enabled MCP server has no loaded tools, indicating
    potential connection or configuration issues.

    Args:
        servers: Dict of {server_name: {"type": str, "enabled": bool}}
                 from ExtensionsConfig.get_enabled_mcp_servers().
        mcp_tools: List of MCP tools from get_cached_mcp_tools().

    Returns:
        List of warning messages for unavailable tools.
    """
    warnings: list[str] = []

    # Get set of servers that have tools loaded
    loaded_servers: set[str] = set()
    for tool in mcp_tools:
        # Parse server name from tool name: "mcp__{server}__{tool}"
        parts = tool.name.split("__")
        if len(parts) >= 2 and parts[0] == "mcp":
            loaded_servers.add(parts[1])

    # Check enabled servers without tools
    for name, config in servers.items():
        if config.get("enabled", True) and name not in loaded_servers:
            msg = f"MCP server '{name}' enabled but no tools loaded - check server logs"
            warnings.append(msg)
            print(f"\n[WARNING] {msg}", file=sys.stderr, flush=True)

    return warnings
