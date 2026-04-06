"""
Duchayuan (都察院) Agent for security audit tasks.

Duchayuan (Censorate) - Responsible for:
- Security vulnerability scanning
- Hardcoded secrets detection
- Dependency vulnerability checking
- Security report generation

Security: Scans are read-only operations, but results may require action.
"""

from typing import TypedDict, Annotated, List, Dict, Any
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info


class DuchayuanState(TypedDict):
    """State for Duchayuan (Censorate) security audit agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    audits_run: List[str]
    vulnerabilities_found: List[Dict[str, Any]]
    errors: List[str]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_security_scan(path: str = "src/", severity: str = "all") -> str:
    """
    Run security vulnerability scan using bandit.

    Args:
        path: Directory or file path to scan
        severity: Minimum severity level (all, low, medium, high, critical)

    Returns:
        JSON string with scan results
    """
    from src.tools.security_audit import security_scan
    return security_scan(path, severity)


@tool
def tool_check_secrets(path: str = "src/") -> str:
    """
    Scan files for hardcoded secrets and credentials.

    Args:
        path: Directory or file path to scan

    Returns:
        JSON string with detected secrets
    """
    from src.tools.security_audit import check_secrets
    return check_secrets(path)


@tool
def tool_check_dependencies(path: str = ".") -> str:
    """
    Check dependencies for known vulnerabilities using pip-audit.

    Args:
        path: Project root path containing requirements.txt or pyproject.toml

    Returns:
        JSON string with vulnerability findings
    """
    from src.tools.security_audit import check_dependencies
    return check_dependencies(path)


@tool
def tool_generate_report(path: str = "src/") -> str:
    """
    Generate comprehensive security report combining all checks.

    Args:
        path: Directory to scan

    Returns:
        JSON string with complete security report
    """
    from src.tools.security_audit import generate_security_report
    return generate_security_report(path)


# Tool set for Duchayuan
duchayuan_tools = [
    tool_security_scan,
    tool_check_secrets,
    tool_check_dependencies,
    tool_generate_report
]


def react_node(state: DuchayuanState):
    """React node for Duchayuan Agent."""
    llm = get_llm("duchayuan_audit")

    # Build system prompt
    sys_prompt = f"""
You are Duchayuan (都察院), the Censorate - a security audit specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Audits run so far: {state.get("audits_run", [])}
Vulnerabilities found: {len(state.get("vulnerabilities_found", []))}

Instructions:
1. Use tool_security_scan to run bandit vulnerability scan
2. Use tool_check_secrets to detect hardcoded credentials
3. Use tool_check_dependencies to check for vulnerable dependencies
4. Use tool_generate_report for comprehensive security report
5. Analyze results and provide actionable recommendations

Security Best Practices:
- Never expose actual secret values in reports (they are masked)
- Prioritize HIGH and CRITICAL severity issues
- Check both source code and configuration files
- Consider false positives and validate findings
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, duchayuan_tools, messages)

    return {"messages": [response]}


def tool_node(state: DuchayuanState):
    """Execute tools for Duchayuan Agent."""
    last_message = state["messages"][-1]
    results = []

    audits_run = state.get("audits_run", []).copy()
    vulnerabilities_found = state.get("vulnerabilities_found", []).copy()
    errors = state.get("errors", []).copy()

    for tool_call in last_message.tool_calls:
        tool_name = tool_call["name"]
        args = tool_call["args"]

        try:
            if tool_name == "tool_security_scan":
                res = tool_security_scan.invoke(args)
                if "security_scan" not in audits_run:
                    audits_run.append("security_scan")
                # Parse vulnerabilities from result
                try:
                    data = json.loads(res)
                    for vuln in data.get("vulnerabilities", []):
                        vulnerabilities_found.append({
                            "source": "bandit",
                            **vuln
                        })
                except (json.JSONDecodeError, TypeError):
                    pass

            elif tool_name == "tool_check_secrets":
                res = tool_check_secrets.invoke(args)
                if "secrets_check" not in audits_run:
                    audits_run.append("secrets_check")
                # Parse secrets from result
                try:
                    data = json.loads(res)
                    for secret in data.get("secrets_found", []):
                        vulnerabilities_found.append({
                            "source": "secrets",
                            **secret
                        })
                except (json.JSONDecodeError, TypeError):
                    pass

            elif tool_name == "tool_check_dependencies":
                res = tool_check_dependencies.invoke(args)
                if "dependency_check" not in audits_run:
                    audits_run.append("dependency_check")
                # Parse dependency vulnerabilities from result
                try:
                    data = json.loads(res)
                    for vuln in data.get("vulnerabilities", []):
                        vulnerabilities_found.append({
                            "source": "pip-audit",
                            **vuln
                        })
                except (json.JSONDecodeError, TypeError):
                    pass

            elif tool_name == "tool_generate_report":
                res = tool_generate_report.invoke(args)
                if "full_report" not in audits_run:
                    audits_run.append("full_report")

            else:
                res = f"Unknown tool: {tool_name}"

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
        "audits_run": audits_run,
        "vulnerabilities_found": vulnerabilities_found,
        "errors": errors
    }


def build_duchayuan_graph():
    """
    Build the Duchayuan Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(DuchayuanState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: DuchayuanState):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
