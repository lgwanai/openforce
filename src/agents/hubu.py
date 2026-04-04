from typing import TypedDict, Annotated, List, Dict, Any, Optional
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os
import asyncio
from concurrent.futures import ThreadPoolExecutor

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, web_search, fetch_webpage, run_agent_browser, run_baidu_search_skill


class HubuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    searched_queries: List[str]
    visited_urls: List[str]
    pending_question: Optional[str]
    context: Dict[str, Any]
    # For parallel execution tracking
    parallel_results: List[Dict[str, Any]]


def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)


@tool
def tool_web_search(query: str) -> str:
    """Search the web for information using Baidu Search API skill."""
    return run_baidu_search_skill(query)


@tool
def tool_fetch_webpage(url: str) -> str:
    """Fetch and read the content of a webpage"""
    return fetch_webpage(url)


@tool
def tool_agent_browser(command: str) -> str:
    """
    Advanced browser automation tool using agent-browser CLI.
    Supports complex interactions like clicking, typing, taking snapshots, and executing JS.
    """
    return run_agent_browser(command)


@tool
def tool_parallel_search(queries: str) -> str:
    """Execute multiple web searches in parallel.

    Args:
        queries: JSON array of search queries, e.g. '["北京天气", "上海天气"]'

    Returns:
        JSON string with all search results
    """
    try:
        query_list = json.loads(queries)
        if not isinstance(query_list, list):
            query_list = [queries]
    except:
        query_list = [queries]

    results = []
    with ThreadPoolExecutor(max_workers=5) as executor:
        futures = {executor.submit(run_baidu_search_skill, q): q for q in query_list}
        for future in futures:
            try:
                result = future.result(timeout=30)
                results.append({"query": futures[future], "result": result})
            except Exception as e:
                results.append({"query": futures[future], "error": str(e)})

    return json.dumps(results, ensure_ascii=False)


@tool
def tool_parallel_fetch(urls: str) -> str:
    """Fetch multiple webpages in parallel.

    Args:
        urls: JSON array of URLs, e.g. '["https://url1.com", "https://url2.com"]'

    Returns:
        JSON string with all webpage contents
    """
    try:
        url_list = json.loads(urls)
        if not isinstance(url_list, list):
            url_list = [urls]
    except:
        url_list = [urls]

    results = []
    with ThreadPoolExecutor(max_workers=5) as executor:
        futures = {executor.submit(fetch_webpage, url): url for url in url_list}
        for future in futures:
            try:
                result = future.result(timeout=30)
                results.append({"url": futures[future], "content": result})
            except Exception as e:
                results.append({"url": futures[future], "error": str(e)})

    return json.dumps(results, ensure_ascii=False)


@tool
def ask_user(question: str) -> str:
    """Ask the user a question to gather missing information."""
    return f"NEED_USER_INPUT: {question}"


# Tools for different modes
basic_tools = [tool_web_search, tool_fetch_webpage, tool_agent_browser, ask_user]
parallel_tools = [tool_parallel_search, tool_parallel_fetch, tool_web_search, tool_fetch_webpage, tool_agent_browser, ask_user]


def react_node(state: HubuState):
    """React node that can plan parallel or sequential tool calls."""
    llm = get_llm("hubu_research")

    context_info = ""
    if state.get("context"):
        context_info = f"\n【已收集的信息】\n{json.dumps(state['context'], ensure_ascii=False, indent=2)}\n"

    pending = state.get("pending_question")
    pending_info = ""
    if pending:
        pending_info = f"\n【等待用户回答】\n问题：{pending}\n"

    parallel_hint = """
【并行执行提示】
如果需要执行多个独立的搜索或抓取任务，请使用并行工具：
- tool_parallel_search: 同时搜索多个关键词
- tool_parallel_fetch: 同时抓取多个网页

这样可以大幅提升效率。只有在任务之间有依赖关系时才使用串行执行。
"""

    sys_prompt = load_prompt(
        "hubu_research",
        current_time=get_current_time(),
        goal=state["goal"],
        searched_queries=json.dumps(state.get("searched_queries", []), ensure_ascii=False),
        visited_urls=json.dumps(state.get("visited_urls", []), ensure_ascii=False),
        context_info=context_info,
        pending_info=pending_info,
        parallel_hint=parallel_hint
    )

    messages = [SystemMessage(content=sys_prompt)] + state["messages"]

    # Use parallel tools by default for efficiency
    response = invoke_llm_with_tools(llm, parallel_tools, messages)

    return {"messages": [response]}


def tool_node(state: HubuState):
    """Execute tools, supporting both single and batch calls."""
    last_message = state["messages"][-1]
    results = []

    searched_queries = state.get("searched_queries", [])
    visited_urls = state.get("visited_urls", [])
    context = state.get("context", {})
    pending_question = None

    for tool_call in last_message.tool_calls:
        tool_name = tool_call["name"]
        args = tool_call["args"]

        try:
            if tool_name == "tool_web_search":
                res = tool_web_search.invoke(args)
                searched_queries.append(args.get("query", ""))

            elif tool_name == "tool_fetch_webpage":
                res = tool_fetch_webpage.invoke(args)
                visited_urls.append(args.get("url", ""))

            elif tool_name == "tool_agent_browser":
                res = tool_agent_browser.invoke(args)

            elif tool_name == "tool_parallel_search":
                res = tool_parallel_search.invoke(args)
                # Extract queries for tracking
                try:
                    queries = json.loads(args.get("queries", "[]"))
                    searched_queries.extend(queries if isinstance(queries, list) else [queries])
                except:
                    pass

            elif tool_name == "tool_parallel_fetch":
                res = tool_parallel_fetch.invoke(args)
                # Extract URLs for tracking
                try:
                    urls = json.loads(args.get("urls", "[]"))
                    visited_urls.extend(urls if isinstance(urls, list) else [urls])
                except:
                    pass

            elif tool_name == "ask_user":
                pending_question = args.get("question", "")
                res = f"已向用户提问：{pending_question}"

            else:
                res = f"Tool {tool_name} not found"

        except Exception as e:
            res = f"Error executing {tool_name}: {str(e)}"

        results.append(ToolMessage(
            content=str(res),
            name=tool_name,
            tool_call_id=tool_call["id"]
        ))

    new_state = {
        "messages": results,
        "searched_queries": searched_queries,
        "visited_urls": visited_urls,
        "context": context,
    }

    if pending_question:
        new_state["pending_question"] = pending_question

    return new_state


def build_hubu_graph():
    workflow = StateGraph(HubuState)

    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)

    workflow.set_entry_point("react")

    def should_continue(state: HubuState):
        if state.get("pending_question"):
            return END

        last_message = state["messages"][-1]
        if getattr(last_message, "tool_calls", None):
            return "tools"
        return END

    def should_continue_after_tools(state: HubuState):
        if state.get("pending_question"):
            return END
        return "react"

    workflow.add_conditional_edges("react", should_continue)
    workflow.add_conditional_edges("tools", should_continue_after_tools)

    return workflow.compile()
