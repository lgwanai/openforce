"""
Xingbu (刑部) Agent for code review and testing.

This agent specializes in:
- Code review with ruff and mypy
- Test execution with pytest
- Coverage analysis with pytest-cov
- Security scanning with bandit
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


class XingbuState(TypedDict):
    """State for Xingbu (Ministry of Justice) code review agent."""
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    files_reviewed: List[str]
    tests_run: List[str]
    issues_found: List[str]
    errors: List[str]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_review_code(filepath: str, focus: str = "all") -> str:
    """
    Review code using ruff and mypy.

    Args:
        filepath: Path to the file or directory to review
        focus: Focus area - "all", "style", "types", or "errors"

    Returns:
        Review results or error message
    """
    from src.tools.code_review import review_code
    return review_code(filepath, focus)


@tool
def tool_run_tests(test_path: str = "tests/", pattern: str = "") -> str:
    """
    Run tests using pytest.

    Args:
        test_path: Path to test file or directory
        pattern: Test name pattern to filter (e.g., "test_login")

    Returns:
        Test results or error message
    """
    from src.tools.code_review import run_tests
    return run_tests(test_path, pattern)


@tool
def tool_check_coverage(test_path: str = "tests/", source_path: str = "src/") -> str:
    """
    Check test coverage using pytest-cov.

    Args:
        test_path: Path to test file or directory
        source_path: Path to source code to measure coverage for

    Returns:
        Coverage report or error message
    """
    from src.tools.code_review import check_coverage
    return check_coverage(test_path, source_path)


@tool
def tool_security_scan(path: str = "src/") -> str:
    """
    Run security scan using bandit.

    Args:
        path: Path to scan for security issues

    Returns:
        Security scan results or error message
    """
    from src.tools.code_review import run_security_scan
    return run_security_scan(path)


# Tool set for Xingbu
xingbu_tools = [
    tool_review_code,
    tool_run_tests,
    tool_check_coverage,
    tool_security_scan
]


def react_node(state: XingbuState):
    """React node for Xingbu Agent."""
    llm = get_llm("xingbu_review")

    # Build system prompt
    sys_prompt = f"""
You are Xingbu (刑部, Ministry of Justice), a code review and testing specialist.

Current time: {get_current_time()}
System info: {get_system_info()}

Goal: {state["goal"]}

Files reviewed so far: {state.get("files_reviewed", [])}
Tests run so far: {state.get("tests_run", [])}
Issues found so far: {state.get("issues_found", [])}

Instructions:
1. Use tool_review_code to check code quality with ruff and mypy
2. Use tool_run_tests to execute tests with pytest
3. Use tool_check_coverage to measure test coverage
4. Use tool_security_scan to check for security issues with bandit

Always provide clear summaries of issues found and recommendations.
"""

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    response = invoke_llm_with_tools(llm, xingbu_tools, messages)

    return {"messages": [response]}


def tool_node(state: XingbuState):
    """Execute tools for Xingbu Agent."""
    from src.security.approval_flow import ApprovalRequest
    from src.security.taint_engine import TaintEngine

    last_message = state["messages"][-1]
    results = []

    files_reviewed = state.get("files_reviewed", []).copy()
    tests_run = state.get("tests_run", []).copy()
    issues_found = state.get("issues_found", []).copy()
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
            if tool_name == "tool_review_code":
                res = tool_review_code.invoke(args)
                filepath = args.get("filepath", "")
                if filepath not in files_reviewed:
                    files_reviewed.append(filepath)
                # Track issues found
                if "error" in res.lower() or "issue" in res.lower():
                    issues_found.append(f"review:{filepath}")

            elif tool_name == "tool_run_tests":
                res = tool_run_tests.invoke(args)
                test_path = args.get("test_path", "tests/")
                tests_run.append(test_path)
                # Track test failures
                if "failed" in res.lower() or "error" in res.lower():
                    issues_found.append(f"test:{test_path}")

            elif tool_name == "tool_check_coverage":
                res = tool_check_coverage.invoke(args)

            elif tool_name == "tool_security_scan":
                res = tool_security_scan.invoke(args)
                path = args.get("path", "src/")
                # Track security issues
                if "issue" in res.lower() or "severity" in res.lower():
                    issues_found.append(f"security:{path}")

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
        "files_reviewed": files_reviewed,
        "tests_run": tests_run,
        "issues_found": issues_found,
        "errors": errors
    }


def build_xingbu_graph():
    """
    Build the Xingbu Agent graph.

    Pattern: react -> tools -> react (loop until done)
    """
    workflow = StateGraph(XingbuState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: XingbuState):
        """Check if agent should continue."""
        last_message = state["messages"][-1]

        # Continue if there are tool calls
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")

    return workflow.compile()
