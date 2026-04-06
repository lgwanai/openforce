"""
Libu2 (Ministry of Rites) Agent for documentation generation tasks.

This agent specializes in:
- Extracting docstrings from Python files
- Generating documentation
- Formatting markdown content
- Creating README files

Libu2 (礼部) is responsible for documentation and ceremonial tasks.
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


class Libu2State(TypedDict):
    """State for Libu2 (Ministry of Rites) documentation agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    docs_created: List[str]
    docs_updated: List[str]
    errors: List[str]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_extract_docstrings(filepath: str) -> str:
    """
    Extract all docstrings from a Python file.

    Args:
        filepath: Path to Python file (relative to project root)

    Returns:
        JSON string containing extracted docstrings
    """
    from src.tools.doc_generator import extract_docstrings
    return extract_docstrings(filepath)


@tool
def tool_generate_doc(module_name: str, output_format: str = "markdown") -> str:
    """
    Generate documentation for a module.

    Args:
        module_name: Module name or path (e.g., "src.tools.base")
        output_format: Output format ("markdown" or "json")

    Returns:
        Generated documentation as string
    """
    from src.tools.doc_generator import generate_doc
    return generate_doc(module_name, output_format)


@tool
def tool_format_markdown(content: str, style: str = "standard") -> str:
    """
    Format markdown content according to a specific style.

    Args:
        content: Markdown content to format
        style: Formatting style ("standard", "compact", "expanded")

    Returns:
        Formatted markdown content
    """
    from src.tools.doc_generator import format_markdown
    return format_markdown(content, style)


@tool
def tool_create_readme(title: str, description: str, sections: str = "") -> str:
    """
    Create a README file content.

    Args:
        title: Project title
        description: Project description
        sections: Additional sections as markdown (optional)

    Returns:
        README content as string
    """
    from src.tools.doc_generator import create_readme
    return create_readme(title, description, sections)


# Tool set for Libu2
libu2_tools = [
    tool_extract_docstrings,
    tool_generate_doc,
    tool_format_markdown,
    tool_create_readme
]


def react_node(state: Libu2State):
    """React node for Libu2 Agent."""
    llm = get_llm("libu2_docs")

    # Build system prompt
    sys_prompt = f"""
You are Libu2 (Ministry of Rites), a documentation generation specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Docs created so far: {state.get("docs_created", [])}
Docs updated so far: {state.get("docs_updated", [])}

Instructions:
1. Use tool_extract_docstrings to extract docstrings from Python files
2. Use tool_generate_doc to generate documentation for modules
3. Use tool_format_markdown to format markdown content
4. Use tool_create_readme to create README file content

Always ensure documentation is clear, accurate, and well-formatted.
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, libu2_tools, messages)

    return {"messages": [response]}


def tool_node(state: Libu2State):
    """Execute tools for Libu2 Agent."""
    from src.security.approval_flow import ApprovalRequest
    from src.security.taint_engine import TaintEngine

    last_message = state["messages"][-1]
    results = []

    docs_created = state.get("docs_created", []).copy()
    docs_updated = state.get("docs_updated", []).copy()
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
            if tool_name == "tool_extract_docstrings":
                res = tool_extract_docstrings.invoke(args)

            elif tool_name == "tool_generate_doc":
                res = tool_generate_doc.invoke(args)
                module_name = args.get("module_name", "")
                if module_name and "Error" not in res:
                    docs_created.append(f"doc:{module_name}")

            elif tool_name == "tool_format_markdown":
                res = tool_format_markdown.invoke(args)

            elif tool_name == "tool_create_readme":
                res = tool_create_readme.invoke(args)
                if "Error" not in res:
                    docs_created.append("README.md")

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
        "docs_created": docs_created,
        "docs_updated": docs_updated,
        "errors": errors
    }


def build_libu2_graph():
    """
    Build the Libu2 Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(Libu2State)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: Libu2State):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
