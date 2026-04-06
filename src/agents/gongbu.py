"""
Gongbu (工部) Agent for environment management tasks.

This agent specializes in:
- Creating virtual environments
- Running commands in environments
- Checking environment status
- Listing and removing environments

Security: High-risk operations require user approval via HIL flow.
"""

from typing import TypedDict, Annotated, List
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info


class GongbuState(TypedDict):
    """State for Gongbu (Ministry of Works) environment management agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    environments_created: List[str]
    commands_run: List[str]
    errors: List[str]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_create_env(env_name: str, python_version: str = "3.11") -> str:
    """
    Create a new virtual environment.

    Args:
        env_name: Name for the environment
        python_version: Python version to use (default: 3.11)

    Returns:
        Success or error message
    """
    from src.tools.env_manager import create_env
    return create_env(env_name, python_version)


@tool
def tool_run_command(command: str, env_name: str = "") -> str:
    """
    Run a command in a virtual environment.

    WARNING: This is a high-risk operation that requires user approval.

    Args:
        command: Command to execute
        env_name: Environment name (empty for system Python)

    Returns:
        Command output or error
    """
    from src.tools.env_manager import run_command
    return run_command(command, env_name)


@tool
def tool_check_env(env_name: str) -> str:
    """
    Check the status of a virtual environment.

    Args:
        env_name: Name of the environment to check

    Returns:
        Status information or error
    """
    from src.tools.env_manager import check_env
    return check_env(env_name)


@tool
def tool_list_envs() -> str:
    """
    List all virtual environments.

    Returns:
        JSON string of environment names
    """
    from src.tools.env_manager import list_envs
    return list_envs()


@tool
def tool_remove_env(env_name: str) -> str:
    """
    Remove a virtual environment.

    WARNING: This is a high-risk operation that requires user approval.

    Args:
        env_name: Name of the environment to remove

    Returns:
        Success or error message
    """
    from src.tools.env_manager import remove_env
    return remove_env(env_name)


# Tool set for Gongbu
gongbu_tools = [
    tool_create_env,
    tool_run_command,
    tool_check_env,
    tool_list_envs,
    tool_remove_env
]


def react_node(state: GongbuState):
    """React node for Gongbu Agent."""
    llm = get_llm("gongbu_env")

    # Build system prompt
    sys_prompt = f"""
You are Gongbu (工部), an environment management specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Environments created so far: {state.get("environments_created", [])}
Commands run so far: {state.get("commands_run", [])}

Instructions:
1. Use tool_create_env to create new virtual environments
2. Use tool_run_command to execute commands in environments (requires approval)
3. Use tool_check_env to verify environment status
4. Use tool_list_envs to see all available environments
5. Use tool_remove_env to clean up environments (requires approval)

Always verify environment creation before running commands.
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, gongbu_tools, messages)

    return {"messages": [response]}


def tool_node(state: GongbuState):
    """Execute tools for Gongbu Agent."""
    from src.security.approval_flow import ApprovalRequest
    from src.security.taint_engine import TaintEngine

    last_message = state["messages"][-1]
    results = []

    environments_created = state.get("environments_created", []).copy()
    commands_run = state.get("commands_run", []).copy()
    errors = state.get("errors", []).copy()

    for tool_call in last_message.tool_calls:
        tool_name = tool_call["name"]
        args = tool_call["args"]

        # Check if high-risk tool requires approval
        if tool_name in TaintEngine.HIGH_RISK_TOOLS:
            raise ApprovalRequest.from_tool_call(
                tool_name=tool_name,
                tool_args=args,
                tool_call_id=tool_call["id"],
                task_id=state.get("task_id", ""),
                owner_user_id=state.get("owner_user_id", ""),
                state_snapshot={
                    "task_id": state.get("task_id", ""),
                    "owner_user_id": state.get("owner_user_id", ""),
                    "messages": state.get("messages", []),
                    "goal": state.get("goal", "")
                }
            )

        try:
            if tool_name == "tool_create_env":
                res = tool_create_env.invoke(args)
                env_name = args.get("env_name", "")
                if "Created environment" in res and env_name not in environments_created:
                    environments_created.append(env_name)

            elif tool_name == "tool_run_command":
                res = tool_run_command.invoke(args)
                commands_run.append(args.get("command", "unknown_command"))

            elif tool_name == "tool_check_env":
                res = tool_check_env.invoke(args)

            elif tool_name == "tool_list_envs":
                res = tool_list_envs.invoke(args)

            elif tool_name == "tool_remove_env":
                res = tool_remove_env.invoke(args)

            else:
                res = f"Unknown tool: {tool_name}"

        except ApprovalRequest:
            raise  # Re-raise approval requests

        except Exception as e:
            res = f"Error: {str(e)}"
            errors.append(f"{tool_name}: {str(e)}")

        results.append(ToolMessage(
            content=str(res),
            name=tool_name,
            tool_call_id=tool_call["id"]
        ))

    return {
        "messages": results,
        "environments_created": environments_created,
        "commands_run": commands_run,
        "errors": errors
    }


def build_gongbu_graph():
    """
    Build the Gongbu Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(GongbuState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: GongbuState):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
