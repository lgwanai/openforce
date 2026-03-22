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
from src.tools.orchestration import spawn_agent, update_task_queue, report_status

class ShangshuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    top_level_goal: str
    acceptance_criteria: str
    total_steps: int
    current_step_index: int
    previous_steps_summary: str
    sub_tasks: List[Dict[str, Any]]

def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)

@tool
def tool_spawn_agent(role: str, goal: str, acceptance_criteria: str) -> str:
    """Spawn a sub-agent to execute a task"""
    return spawn_agent(role, goal, acceptance_criteria)

@tool
def tool_update_task_queue(task_id: str, status: str) -> str:
    """Update task queue status"""
    return update_task_queue(task_id, status)

@tool
def tool_report_status(summary: str) -> str:
    """Report status back to upper layer"""
    return report_status(summary)

tools = [tool_spawn_agent, tool_update_task_queue, tool_report_status]

def orchestrate_node(state: ShangshuState):
    llm = get_llm("shangshu_orchestrator")
    
    available_agents = "兵部 (bingbu_coder) - General execution and coding\n户部 (hubu_research) - Information research\n刑部 (xingbu_reviewer) - Review and testing"
    
    sys_prompt = load_prompt(
        "shangshu_orchestrator", 
        current_time=get_current_time(),
        system_info=get_system_info(),
        top_level_goal=state.get("top_level_goal", "N/A"),
        acceptance_criteria=state.get("acceptance_criteria", "N/A"),
        available_agents_description=available_agents,
        total_steps=state.get("total_steps", 0),
        current_step_index=state.get("current_step_index", 0),
        previous_steps_summary=state.get("previous_steps_summary", "None")
    )
    
    messages = [SystemMessage(content=sys_prompt)] + state["messages"]
    response = invoke_llm_with_tools(llm, tools, messages)
    
    return {"messages": [response]}

def tool_node(state: ShangshuState):
    last_message = state["messages"][-1]
    results = []
    
    for tool_call in last_message.tool_calls:
        try:
            if tool_call["name"] == "tool_spawn_agent":
                res = tool_spawn_agent.invoke(tool_call["args"])
                res = f"Agent {tool_call['args'].get('role', 'unknown')} spawned and completed. Result: {res}"
            elif tool_call["name"] == "tool_update_task_queue":
                res = tool_update_task_queue.invoke(tool_call["args"])
            elif tool_call["name"] == "tool_report_status":
                res = tool_report_status.invoke(tool_call["args"])
            else:
                res = f"Tool {tool_call['name']} not found"
        except Exception as e:
            res = f"Error executing tool {tool_call['name']}: {str(e)}"
            
        results.append(ToolMessage(
            content=str(res),
            name=tool_call["name"],
            tool_call_id=tool_call["id"]
        ))
        
    return {"messages": results}

def build_shangshu_graph():
    workflow = StateGraph(ShangshuState)
    
    workflow.add_node("orchestrate", orchestrate_node)
    workflow.add_node("tools", tool_node)
    
    workflow.set_entry_point("orchestrate")
    
    def should_continue(state: ShangshuState):
        last_message = state["messages"][-1]
        if last_message.tool_calls:
            return "tools"
        return END
        
    workflow.add_conditional_edges("orchestrate", should_continue)
    workflow.add_edge("tools", "orchestrate")
    
    return workflow.compile()
