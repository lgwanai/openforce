from typing import TypedDict, Annotated, List, Dict, Any
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, web_search, fetch_webpage, run_agent_browser, run_baidu_search_skill

class HubuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    searched_queries: List[str]
    visited_urls: List[str]

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
    Use this tool especially when local info is missing and you need to explore the web via common search engines (Bing, Baidu, etc.) or when you need to interact with a page.
    IMPORTANT: 'open', 'click', and 'type' do NOT return page content. You must chain them with 'snapshot -i' using '&&' to see the page!
    Examples of command parameter:
      'open "https://www.bing.com/search?q=OpenClaw" && agent-browser snapshot -i'
      'type @e2 "some text" && agent-browser click @e1 && agent-browser snapshot -i'
      'snapshot -i'
    """
    return run_agent_browser(command)

tools = [tool_web_search, tool_fetch_webpage, tool_agent_browser]

def react_node(state: HubuState):
    llm = get_llm("hubu_research")
    
    sys_prompt = load_prompt(
        "hubu_research", 
        current_time=get_current_time(),
        goal=state["goal"],
        searched_queries=json.dumps(state.get("searched_queries", [])),
        visited_urls=json.dumps(state.get("visited_urls", []))
    )
    
    messages = [SystemMessage(content=sys_prompt)] + state["messages"]
    response = invoke_llm_with_tools(llm, tools, messages)
    
    return {"messages": [response]}

def tool_node(state: HubuState):
    last_message = state["messages"][-1]
    results = []
    
    searched_queries = state.get("searched_queries", [])
    visited_urls = state.get("visited_urls", [])
    
    for tool_call in last_message.tool_calls:
        try:
            if tool_call["name"] == "tool_web_search":
                res = tool_web_search.invoke(tool_call["args"])
                searched_queries.append(tool_call["args"].get("query", ""))
            elif tool_call["name"] == "tool_fetch_webpage":
                res = tool_fetch_webpage.invoke(tool_call["args"])
                visited_urls.append(tool_call["args"].get("url", ""))
            elif tool_call["name"] == "tool_agent_browser":
                res = tool_agent_browser.invoke(tool_call["args"])
            else:
                res = f"Tool {tool_call['name']} not found"
        except Exception as e:
            res = f"Error executing tool {tool_call['name']}: {str(e)}"
            
        results.append(ToolMessage(
            content=str(res),
            name=tool_call["name"],
            tool_call_id=tool_call["id"]
        ))
        
    return {
        "messages": results,
        "searched_queries": searched_queries,
        "visited_urls": visited_urls
    }

def build_hubu_graph():
    workflow = StateGraph(HubuState)
    
    workflow.add_node("react", react_node)
    workflow.add_node("tools", tool_node)
    
    workflow.set_entry_point("react")
    
    def should_continue(state: HubuState):
        last_message = state["messages"][-1]
        if last_message.tool_calls:
            return "tools"
        return END
        
    workflow.add_conditional_edges("react", should_continue)
    workflow.add_edge("tools", "react")
    
    return workflow.compile()
