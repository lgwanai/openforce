"""
Bingbu (Ministry of War) Agent for code execution tasks.

This agent specializes in:
- Creating files
- Editing files
- Executing Python code
- Managing project structure

Security: High-risk operations require user approval via HIL flow.
"""

from typing import TypedDict, Annotated, List, Dict, Any, Optional
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info


class BingbuState(TypedDict):
    """State for Bingbu (Ministry of War) code execution agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    files_created: List[str]
    files_modified: List[str]
    commands_executed: List[str]
    errors: List[str]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_create_file(filepath: str, content: str) -> str:
    """
    Create a new file with the given content.

    Args:
        filepath: Path relative to project root
        content: Content to write to the file

    Returns:
        Success or error message
    """
    # Import here to avoid circular dependency
    from src.tools.code_executor import create_file
    return create_file(filepath, content)


@tool
def tool_edit_file(filepath: str, old_content: str, new_content: str) -> str:
    """
    Edit an existing file by replacing old_content with new_content.

    Args:
        filepath: Path relative to project root
        old_content: Exact text to find and replace
        new_content: New text to insert

    Returns:
        Success or error message
    """
    from src.tools.code_executor import edit_file
    return edit_file(filepath, old_content, new_content)


@tool
def tool_read_file(filepath: str) -> str:
    """
    Read the contents of a file.

    Args:
        filepath: Path relative to project root

    Returns:
        File contents or error message
    """
    from src.tools.base import read_file
    return read_file(filepath, sandbox_only=False)


@tool
def tool_list_files(directory: str = ".") -> str:
    """
    List files in a directory.

    Args:
        directory: Directory path relative to project root

    Returns:
        JSON list of files
    """
    from src.tools.base import list_directory
    import json
    result = list_directory(directory, sandbox_only=False)
    return json.dumps(result, ensure_ascii=False)


@tool
def tool_execute_python(code: str) -> str:
    """
    Execute Python code in a sandboxed environment.

    WARNING: This is a high-risk operation that requires user approval.

    Args:
        code: Python code to execute

    Returns:
        Execution output or error message
    """
    from src.tools.code_executor import execute_python
    return execute_python(code)


# Tool set for Bingbu
bingbu_tools = [
    tool_create_file,
    tool_edit_file,
    tool_read_file,
    tool_list_files,
    tool_execute_python
]


def react_node(state: BingbuState):
    """React node for Bingbu Agent."""
    llm = get_llm("bingbu_code")

    # Build system prompt
    sys_prompt = f"""
You are Bingbu (Ministry of War), a code execution specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Files created so far: {state.get("files_created", [])}
Files modified so far: {state.get("files_modified", [])}

Instructions:
1. Use tool_create_file to create new files
2. Use tool_edit_file to modify existing files
3. Use tool_read_file to read files before editing
4. Use tool_list_files to explore the project structure
5. Use tool_execute_python for testing (requires approval)

Always verify file contents after creating or editing.
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, bingbu_tools, messages)

    return {"messages": [response]}


def tool_node(state: BingbuState):
    """Execute tools for Bingbu Agent."""
    from src.security.approval_flow import ApprovalRequest
    from src.security.taint_engine import TaintEngine

    last_message = state["messages"][-1]
    results = []

    files_created = state.get("files_created", []).copy()
    files_modified = state.get("files_modified", []).copy()
    commands_executed = state.get("commands_executed", []).copy()
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
            if tool_name == "tool_create_file":
                res = tool_create_file.invoke(args)
                filepath = args.get("filepath", "")
                if "Created file" in res and filepath not in files_created:
                    files_created.append(filepath)

            elif tool_name == "tool_edit_file":
                res = tool_edit_file.invoke(args)
                filepath = args.get("filepath", "")
                if "Edited file" in res and filepath not in files_modified:
                    files_modified.append(filepath)

            elif tool_name == "tool_read_file":
                res = tool_read_file.invoke(args)

            elif tool_name == "tool_list_files":
                res = tool_list_files.invoke(args)

            elif tool_name == "tool_execute_python":
                res = tool_execute_python.invoke(args)
                commands_executed.append("python_execution")

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
        "files_created": files_created,
        "files_modified": files_modified,
        "commands_executed": commands_executed,
        "errors": errors
    }


def build_bingbu_graph():
    """
    Build the Bingbu Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(BingbuState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: BingbuState):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
