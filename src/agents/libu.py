"""
Libu (Ministry of Personnel) Agent for skill management tasks.

This agent specializes in:
- Installing skills (Python packages)
- Updating skills
- Listing installed skills
- Uninstalling skills

Security: Package operations require user approval via HIL flow.
"""

from typing import TypedDict, Annotated, List
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info


class LibuState(TypedDict):
    """State for Libu (Ministry of Personnel) skill management agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    skills_installed: List[str]
    skills_updated: List[str]
    errors: List[str]


@tool
def tool_install_skill(skill_name: str, source: str = "pip") -> str:
    """
    Install a skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to install
        source: Installation source ("pip" or "git")

    Returns:
        Success or error message
    """
    from src.tools.skill_manager import install_skill
    return install_skill(skill_name, source)


@tool
def tool_update_skill(skill_name: str) -> str:
    """
    Update an installed skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to update

    Returns:
        Success or error message
    """
    from src.tools.skill_manager import update_skill
    return update_skill(skill_name)


@tool
def tool_list_skills() -> str:
    """
    List all installed Python packages/skills.

    Returns:
        JSON string listing all installed packages
    """
    from src.tools.skill_manager import list_skills
    return list_skills()


@tool
def tool_uninstall_skill(skill_name: str) -> str:
    """
    Uninstall a skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to uninstall

    Returns:
        Success or error message
    """
    from src.tools.skill_manager import uninstall_skill
    return uninstall_skill(skill_name)


# Tool set for Libu
libu_tools = [
    tool_install_skill,
    tool_update_skill,
    tool_list_skills,
    tool_uninstall_skill
]


def react_node(state: LibuState):
    """React node for Libu Agent."""
    llm = get_llm("libu_skill")

    # Build system prompt
    sys_prompt = f"""
You are Libu (Ministry of Personnel), a skill management specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Skills installed so far: {state.get("skills_installed", [])}
Skills updated so far: {state.get("skills_updated", [])}

Instructions:
1. Use tool_install_skill to install new packages/skills
2. Use tool_update_skill to update existing packages
3. Use tool_list_skills to see what's installed
4. Use tool_uninstall_skill to remove packages

Always verify installation success after operations.
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, libu_tools, messages)

    return {"messages": [response]}


def tool_node(state: LibuState):
    """Execute tools for Libu Agent."""
    from src.security.approval_flow import ApprovalRequest
    from src.security.taint_engine import TaintEngine

    last_message = state["messages"][-1]
    results = []

    skills_installed = state.get("skills_installed", []).copy()
    skills_updated = state.get("skills_updated", []).copy()
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
            if tool_name == "tool_install_skill":
                res = tool_install_skill.invoke(args)
                skill_name = args.get("skill_name", "")
                if "Successfully installed" in res and skill_name not in skills_installed:
                    skills_installed.append(skill_name)

            elif tool_name == "tool_update_skill":
                res = tool_update_skill.invoke(args)
                skill_name = args.get("skill_name", "")
                if "Successfully updated" in res and skill_name not in skills_updated:
                    skills_updated.append(skill_name)

            elif tool_name == "tool_list_skills":
                res = tool_list_skills.invoke(args)

            elif tool_name == "tool_uninstall_skill":
                res = tool_uninstall_skill.invoke(args)

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
        "skills_installed": skills_installed,
        "skills_updated": skills_updated,
        "errors": errors
    }


def build_libu_graph():
    """
    Build the Libu Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(LibuState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: LibuState):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
