from typing import TypedDict, Annotated, List, Dict, Any
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info, read_file, write_file, list_directory, get_current_path
from src.agents.shangshu import build_shangshu_graph
from src.agents.hubu import build_hubu_graph

class ZhongshuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    owner_user_id: str
    intent: str
    plan: Dict[str, Any]
    
def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)

# Decorate base tools
@tool
def tool_read_file(filepath: str) -> str:
    """Read file content. Always use absolute paths or paths relative to sandbox root."""
    return read_file(filepath)

@tool
def tool_write_file(filepath: str, content: str) -> str:
    """Write content to file."""
    return write_file(filepath, content)

@tool
def tool_list_directory(path: str) -> str:
    """List directory. Provide a path like '/' or '.' to list current directory."""
    return list_directory(path)

@tool
def tool_get_current_path(dummy: str = "") -> str:
    """Get current working directory inside the sandbox."""
    return get_current_path()

@tool
def delegate_to_shangshu(plan: str) -> str:
    """Delegate complex tasks (like writing code, creating projects) to Shangshu. Provide a detailed plan."""
    return "Delegated"

@tool
def delegate_to_hubu(research_goal: str) -> str:
    """Delegate deep research, web search, or complex knowledge QA to Hubu."""
    return "Delegated"

tools = [tool_read_file, tool_write_file, tool_list_directory, tool_get_current_path, delegate_to_shangshu, delegate_to_hubu]

def router_node(state: ZhongshuState):
    llm = get_llm("zhongshu_planner")
    
    sys_prompt = load_prompt(
        "zhongshu_system", 
        current_time=get_current_time(),
        system_info=get_system_info()
    )
    
    # Check if there are tool messages in history to prevent infinite loops
    # If the last message is a ToolMessage, we should force the LLM to analyze it and respond to user
    messages = [SystemMessage(content=sys_prompt)]
    
    # Langchain expects strictly alternating Human/AI or Human/AI/Tool/AI patterns
    # Just pass all messages directly
    messages.extend(state["messages"])
    
    # We can pass an instruction to force standard response if needed
    # Instead of appending a HumanMessage which might break the alternation rule,
    # we can append a dummy AI message and a human message if needed, or just let Langchain handle it.
    # Actually, appending a ToolMessage without its corresponding AIMessage might cause errors in some APIs.
    # The state["messages"] should naturally contain the sequence.
    # Let's just pass the messages.
    
    # Check if last message is a ToolMessage, maybe we shouldn't prompt tools again
    # We shouldn't append HumanMessage directly to state array. 
    # Just invoke llm with standard messages. The LLM should naturally read the ToolMessage.
    
    # Check if the last message is a ToolMessage
    if len(state["messages"]) > 0 and isinstance(state["messages"][-1], ToolMessage):
        tool_msg = state["messages"][-1]
        
        # Determine intent for the tool response
        # If the tool was delegate_to_shangshu, it's a Task.
        if tool_msg.name == "delegate_to_shangshu":
            return {"messages": [], "intent": "Task"}
        elif tool_msg.name == "delegate_to_hubu":
            return {"messages": [], "intent": "QA"}
            
        # We construct a response without invoking the LLM to avoid 500 error
        # since some providers crash when ToolMessage is the last message.
        if tool_msg.name == "tool_get_current_path":
            response = AIMessage(content=f"当前工作目录是：`{tool_msg.content}`")
        elif tool_msg.name == "tool_list_directory":
            response = AIMessage(content=f"目录内容如下：\n```\n{tool_msg.content}\n```")
        elif tool_msg.name == "tool_read_file":
            response = AIMessage(content=f"文件内容如下：\n```\n{tool_msg.content[:1000]}\n```" + ("\n...(已截断)" if len(tool_msg.content) > 1000 else ""))
        elif tool_msg.name == "tool_write_file":
            response = AIMessage(content=f"文件写入成功。\n{tool_msg.content}")
        else:
            response = AIMessage(content=f"工具 `{tool_msg.name}` 执行完毕。\n结果：\n{tool_msg.content}")
            
        return {"messages": [response], "intent": "Chat"}
        
    try:
        response = invoke_llm_with_tools(llm, tools, messages)
    except Exception as e:
        # Instead of just an AIMessage, log the actual exception and set tool_calls to none
        response = AIMessage(content=f"LLM Error: {str(e)}")
        return {"messages": [response], "intent": "Chat"}
    
    # If this response is just an AIMessage acknowledging the tool but it has no content,
    # we might want to force some text so it doesn't get swallowed.
    content_str = str(getattr(response, "content", ""))
    if not content_str.strip() and not getattr(response, "tool_calls", None):
        # Check if the model tried to call a tool but it was swallowed due to format error
        finish_reason = getattr(response, "response_metadata", {}).get("finish_reason", "")
        if finish_reason == "tool_calls":
            # For Minimax and other providers, sometimes the tool_calls are lost due to strict parsing.
            # We can prompt the model to try again with a clear instruction.
            response = AIMessage(content="[系统提示] 大模型尝试调用工具，但生成的工具调用格式无效（如 JSON 截断或遗漏必填参数），导致调用被忽略。这可能是因为模型输出了非标准的函数参数。请稍后重试。")
            return {"messages": [response], "intent": "Chat"}
            
        # If still no content and no tool calls, but we had a ToolMessage before,
        # it means the model is just returning empty. We should force a response.
        tool_msg = next((m for m in reversed(state["messages"]) if isinstance(m, ToolMessage)), None)
        if tool_msg:
            response = AIMessage(content=f"根据工具返回结果：\n{tool_msg.content}")
            return {"messages": [response], "intent": "Chat"}
        else:
            response = AIMessage(content="我已完成处理。")
        
    # Determine intent based on tool calls
    if getattr(response, "tool_calls", None):
        for tc in response.tool_calls:
            if tc["name"] == "delegate_to_shangshu":
                return {"messages": [response], "intent": "Task"}
            elif tc["name"] == "delegate_to_hubu":
                return {"messages": [response], "intent": "QA"}
        return {"messages": [response], "intent": "Basic Tool"}
    else:
        return {"messages": [response], "intent": "Chat"}

def tool_node(state: ZhongshuState):
    last_message = state["messages"][-1]
    results = []
    
    # We must have tool calls to process
    if not hasattr(last_message, "tool_calls") or not last_message.tool_calls:
        return {"messages": []}
        
    for tool_call in last_message.tool_calls:
        try:
            if tool_call["name"] == "tool_read_file":
                res = tool_read_file.invoke(tool_call["args"])
            elif tool_call["name"] == "tool_write_file":
                res = tool_write_file.invoke(tool_call["args"])
            elif tool_call["name"] == "tool_list_directory":
                res = tool_list_directory.invoke(tool_call["args"])
            elif tool_call["name"] == "tool_get_current_path":
                res = tool_get_current_path.invoke(tool_call["args"])
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

def delegate_node(state: ZhongshuState):
    intent = state.get("intent")
    last_message = state["messages"][-1]
    
    # Extract plan or goal from tool call if present
    goal_text = ""
    if getattr(last_message, "tool_calls", None):
        for tc in last_message.tool_calls:
            if tc["name"] == "delegate_to_shangshu":
                goal_text = tc["args"].get("plan", "")
            elif tc["name"] == "delegate_to_hubu":
                goal_text = tc["args"].get("research_goal", "")
                
    # Fallback to human message if extraction failed
    if not goal_text:
        human_msg = next((m for m in reversed(state["messages"]) if isinstance(m, HumanMessage)), None)
        goal_text = human_msg.content if human_msg else "Auto-delegated task"
        
    if intent == "Task":
        shangshu_graph = build_shangshu_graph()
        shangshu_state = {
            "messages": [HumanMessage(content=goal_text)],
            "task_id": state.get("task_id", ""),
            "top_level_goal": goal_text,
            "acceptance_criteria": "Auto-generated",
            "total_steps": 1,
            "current_step_index": 1,
            "previous_steps_summary": "None",
            "sub_tasks": []
        }
        res = shangshu_graph.invoke(shangshu_state)
        final_msg = res["messages"][-1].content
        
        # Create ToolMessage to satisfy API requirements
        tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
        tool_msg = ToolMessage(content="Delegated successfully", name="delegate_to_shangshu", tool_call_id=tool_call_id)
        
        return {"messages": [tool_msg, AIMessage(content=f"[Delegated to Shangshu] {final_msg}")]}
        
    elif intent == "QA":
        hubu_graph = build_hubu_graph()
        hubu_state = {
            "messages": [HumanMessage(content=goal_text)],
            "task_id": state.get("task_id", ""),
            "goal": goal_text,
            "searched_queries": [],
            "visited_urls": []
        }
        res = hubu_graph.invoke(hubu_state)
        final_msg = res["messages"][-1].content
        
        tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
        tool_msg = ToolMessage(content="Delegated successfully", name="delegate_to_hubu", tool_call_id=tool_call_id)
        
        return {"messages": [tool_msg, AIMessage(content=f"[Delegated to Hubu] {final_msg}")]}
        
    return {"messages": []}

def build_zhongshu_graph():
    workflow = StateGraph(ZhongshuState)
    
    workflow.add_node("router", router_node)
    workflow.add_node("tools", tool_node)
    workflow.add_node("delegate", delegate_node)
    
    workflow.set_entry_point("router")
    
    def should_continue(state: ZhongshuState):
        intent = state.get("intent")
        last_message = state["messages"][-1]
        
        # Check intent first to route to delegate
        if intent in ["Task", "QA"]:
            return "delegate"
            
        # If the last message has tool calls (and not delegated), go to tools
        if getattr(last_message, "tool_calls", None):
            return "tools"
            
        return END
        
    workflow.add_conditional_edges("router", should_continue)
    
    # After tools are executed, go back to router to analyze tool results and give final answer
    workflow.add_edge("tools", "router")
    workflow.add_edge("delegate", END)
    
    return workflow.compile()
