from typing import TypedDict, Annotated, List, Dict, Any, Optional
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool
import json
import os
import datetime

from src.core.config import get_llm
from src.core.utils import invoke_llm_with_tools
from src.tools.base import get_current_time, get_system_info, read_file, write_file, list_directory, get_current_path
from src.agents.shangshu import build_shangshu_graph
from src.agents.hubu import build_hubu_graph
from src.agents.bingbu import build_bingbu_graph
from src.security.approval_flow import ApprovalRequest
from src.security.taint_engine import TaintEngine

class ZhongshuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    owner_user_id: str
    intent: str
    plan: Dict[str, Any]
    # For context continuation
    pending_delegate_state: Optional[Dict[str, Any]]
    # For intermediate feedback from delegates
    intermediate_status: Optional[Dict[str, Any]]

def load_prompt(template_name: str, **kwargs) -> str:
    path = os.path.join(os.path.dirname(__file__), f"../../prompts/{template_name}.md")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return content.format(**kwargs)

# Decorate base tools - allow project access (will trigger approval for non-sandbox paths)
@tool
def tool_read_file(filepath: str) -> str:
    """Read file content. Can access project files with user approval."""
    return read_file(filepath, sandbox_only=False)

@tool
def tool_write_file(filepath: str, content: str) -> str:
    """Write content to file. Can access project files with user approval."""
    return write_file(filepath, content, sandbox_only=False)

@tool
def tool_list_directory(path: str = ".") -> str:
    """List directory contents. Can access project directories with user approval.
    Use '.' to list current project root directory."""
    result = list_directory(path, sandbox_only=False)
    return json.dumps(result, ensure_ascii=False)

@tool
def tool_get_current_path(dummy: str = "") -> str:
    """Get the current project root directory."""
    return get_current_path()

@tool
def delegate_to_shangshu(plan: str) -> str:
    """Delegate complex tasks (like writing code, creating projects) to Shangshu. Provide a detailed plan."""
    return "Delegated"

@tool
def delegate_to_hubu(research_goal: str) -> str:
    """Delegate deep research, web search, or complex knowledge QA to Hubu."""
    return "Delegated"

@tool
def resolve_relative_time(time_expression: str) -> str:
    """Convert relative time expressions to absolute date/time.

    Args:
        time_expression: Natural language time expression like "明天", "后天", "下周三", "去年", "上周一"

    Returns:
        JSON string with resolved absolute date and original expression
    """
    import re

    now = datetime.datetime.now()
    result = {
        "original": time_expression,
        "current_time": now.strftime("%Y-%m-%d %H:%M:%S"),
        "resolved_date": None,
        "resolved_datetime": None
    }

    expr = time_expression.strip()

    # Handle common patterns
    if expr in ["今天", "今日"]:
        result["resolved_date"] = now.strftime("%Y-%m-%d")
        result["resolved_datetime"] = now.strftime("%Y-%m-%d %H:%M:%S")
    elif expr in ["明天", "明日"]:
        target = now + datetime.timedelta(days=1)
        result["resolved_date"] = target.strftime("%Y-%m-%d")
        result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
    elif expr in ["后天"]:
        target = now + datetime.timedelta(days=2)
        result["resolved_date"] = target.strftime("%Y-%m-%d")
        result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
    elif expr in ["大后天"]:
        target = now + datetime.timedelta(days=3)
        result["resolved_date"] = target.strftime("%Y-%m-%d")
        result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
    elif expr in ["昨天", "昨日"]:
        target = now - datetime.timedelta(days=1)
        result["resolved_date"] = target.strftime("%Y-%m-%d")
        result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
    elif expr in ["前天"]:
        target = now - datetime.timedelta(days=2)
        result["resolved_date"] = target.strftime("%Y-%m-%d")
        result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
    elif expr == "去年":
        result["resolved_date"] = f"{now.year - 1}年"
        result["resolved_datetime"] = None
    elif expr == "今年":
        result["resolved_date"] = f"{now.year}年"
        result["resolved_datetime"] = None
    elif expr == "上个月":
        if now.month == 1:
            result["resolved_date"] = f"{now.year - 1}年12月"
        else:
            result["resolved_date"] = f"{now.year}年{now.month - 1}月"
    elif expr == "下个月":
        if now.month == 12:
            result["resolved_date"] = f"{now.year + 1}年1月"
        else:
            result["resolved_date"] = f"{now.year}年{now.month + 1}月"
    else:
        # Try to match patterns like "下周三", "上周一"
        weekdays = ["一", "二", "三", "四", "五", "六", "日"]
        week_match = re.match(r"下周([一二三四五六日天])", expr)
        if week_match:
            target_day = weekdays.index(week_match.group(1)) if week_match.group(1) != "天" else 6
            days_ahead = target_day - now.weekday()
            if days_ahead <= 0:
                days_ahead += 7
            target = now + datetime.timedelta(days=days_ahead)
            result["resolved_date"] = target.strftime("%Y-%m-%d")
            result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
        else:
            week_match = re.match(r"上周([一二三四五六日天])", expr)
            if week_match:
                target_day = weekdays.index(week_match.group(1)) if week_match.group(1) != "天" else 6
                days_behind = now.weekday() - target_day
                if days_behind < 0:
                    days_behind += 7
                target = now - datetime.timedelta(days=days_behind)
                result["resolved_date"] = target.strftime("%Y-%m-%d")
                result["resolved_datetime"] = target.strftime("%Y-%m-%d %H:%M:%S")
            else:
                result["resolved_date"] = "无法解析"
                result["resolved_datetime"] = None

    return json.dumps(result, ensure_ascii=False)

@tool
def delegate_to_bingbu(code_task: str) -> str:
    """Delegate code writing, file creation, or script execution tasks to Bingbu."""
    return "Delegated"

tools = [tool_read_file, tool_write_file, tool_list_directory, tool_get_current_path, resolve_relative_time, delegate_to_shangshu, delegate_to_hubu, delegate_to_bingbu]

def router_node(state: ZhongshuState):
    llm = get_llm("zhongshu_planner")

    sys_prompt = load_prompt(
        "zhongshu_system",
        current_time=get_current_time(),
        system_info=get_system_info()
    )

    last_message = state["messages"][-1] if state["messages"] else None

    # Check if we're continuing a pending delegate task
    pending_delegate_state = state.get("pending_delegate_state")
    if pending_delegate_state and isinstance(last_message, HumanMessage):
        return {"messages": [], "intent": "Continue"}

    # Check if delegate returned a question for user (from pending state continuation)
    if isinstance(last_message, AIMessage) and last_message.additional_kwargs.get("is_question"):
        # This is a question for the user, just return it
        return {"messages": [], "intent": "Chat"}

    # Check if delegate returned a result to format
    if isinstance(last_message, AIMessage) and last_message.additional_kwargs.get("is_hubu_result"):
        # Hubu returned data, format it for user
        original_question = None
        for msg in reversed(state["messages"]):
            if isinstance(msg, HumanMessage):
                original_question = msg.content
                break

        process_messages = [
            SystemMessage(content=sys_prompt),
            SystemMessage(content=(
                "户部已完成信息收集。请根据用户的原始问题和收集到的信息，生成一个清晰、有帮助的回复。\n\n"
                "重要规则：\n"
                "1. 你是在和真人对话，请用自然、友好的语言交流\n"
                "2. 根据用户的问题整理信息，生成有帮助的回复\n"
                "3. 不要输出任何工具调用格式\n"
            ))
        ]

        if original_question:
            process_messages.append(HumanMessage(content=f"用户问题：{original_question}"))

        process_messages.append(HumanMessage(content=f"户部收集的信息：\n{last_message.content}"))

        try:
            response = llm.invoke(process_messages)
            return {"messages": [response], "intent": "Chat"}
        except Exception as e:
            return {"messages": [AIMessage(content=last_message.content)], "intent": "Chat"}

    # Check if the last message is a ToolMessage (basic tool execution result)
    if isinstance(last_message, ToolMessage):
        tool_msg = last_message

        # Check if hubu needs user input (from pending_delegate_state)
        pending = state.get("pending_delegate_state")
        if pending and pending.get("question"):
            # Hubu is asking a question, return it to user
            return {"messages": [AIMessage(content=pending["question"])], "intent": "Chat"}

        # Determine intent for delegation tools
        if tool_msg.name == "delegate_to_shangshu":
            return {"messages": [], "intent": "Task"}
        elif tool_msg.name == "delegate_to_hubu":
            # Hubu returned result, format it for user
            original_question = None
            for msg in reversed(state["messages"]):
                if isinstance(msg, HumanMessage):
                    original_question = msg.content
                    break

            process_messages = [
                SystemMessage(content=sys_prompt),
                SystemMessage(content=(
                    "户部已完成信息收集。请根据用户的原始问题和收集到的信息，生成一个清晰、有帮助的回复。\n\n"
                    "重要规则：\n"
                    "1. 你是在和真人对话，请用自然、友好的语言交流\n"
                    "2. 根据用户的问题整理信息，生成有帮助的回复\n"
                    "3. 不要输出任何工具调用格式\n"
                ))
            ]

            if original_question:
                process_messages.append(HumanMessage(content=f"用户问题：{original_question}"))

            process_messages.append(HumanMessage(content=f"户部收集的信息：\n{tool_msg.content}"))

            try:
                response = llm.invoke(process_messages)
                return {"messages": [response], "intent": "Chat"}
            except Exception as e:
                return {"messages": [AIMessage(content=tool_msg.content)], "intent": "Chat"}

        # For basic tools (read_file, list_directory, etc.)
        original_question = None
        for msg in reversed(state["messages"]):
            if isinstance(msg, HumanMessage):
                original_question = msg.content
                break

        process_messages = [
            SystemMessage(content=sys_prompt),
            SystemMessage(content=(
                "工具已执行完毕。现在请直接用自然语言回复用户。\n\n"
                "重要规则：\n"
                "1. 你是在和真人对话，请用自然、友好的语言交流\n"
                "2. 根据用户的问题和工具返回的结果，生成有帮助的回复\n"
                "3. 不要输出任何工具调用格式（如 [TOOL_CALL]、<tool_calls> 等）\n"
                "4. 不要尝试再次调用工具，直接回答用户\n"
            ))
        ]

        if original_question:
            process_messages.append(HumanMessage(content=f"用户问题：{original_question}"))

        process_messages.append(HumanMessage(content=f"工具 {tool_msg.name} 返回结果：\n{tool_msg.content}"))

        try:
            response = llm.invoke(process_messages)
            return {"messages": [response], "intent": "Chat"}
        except Exception as e:
            response = AIMessage(content=f"工具执行完成。结果：\n{tool_msg.content}")
            return {"messages": [response], "intent": "Chat"}

    # Normal flow: process user input
    messages = [SystemMessage(content=sys_prompt)]
    messages.extend(state["messages"])

    try:
        response = invoke_llm_with_tools(llm, tools, messages)
    except Exception as e:
        response = AIMessage(content=f"LLM Error: {str(e)}")
        return {"messages": [response], "intent": "Chat"}

    # Handle empty response with tool_calls finish reason
    content_str = str(getattr(response, "content", ""))
    if not content_str.strip() and not getattr(response, "tool_calls", None):
        finish_reason = getattr(response, "response_metadata", {}).get("finish_reason", "")
        if finish_reason == "tool_calls":
            response = AIMessage(content="[系统提示] 工具调用格式无效，请稍后重试。")
            return {"messages": [response], "intent": "Chat"}

        # Fallback for empty response
        tool_msg = next((m for m in reversed(state["messages"]) if isinstance(m, ToolMessage)), None)
        if tool_msg:
            response = AIMessage(content=f"处理完成。结果：\n{tool_msg.content}")
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
            elif tc["name"] == "delegate_to_bingbu":
                return {"messages": [response], "intent": "Code"}
        return {"messages": [response], "intent": "Basic Tool"}
    else:
        return {"messages": [response], "intent": "Chat"}

def tool_node(state: ZhongshuState):
    last_message = state["messages"][-1]
    results = []

    if not hasattr(last_message, "tool_calls") or not last_message.tool_calls:
        return {"messages": []}

    for tool_call in last_message.tool_calls:
        tool_name = tool_call["name"]
        tool_args = tool_call["args"]

        # Check if high-risk tool requires approval
        if tool_name in TaintEngine.HIGH_RISK_TOOLS:
            raise ApprovalRequest.from_tool_call(
                tool_name=tool_name,
                tool_args=tool_args,
                tool_call_id=tool_call["id"],
                task_id=state.get("task_id", ""),
                owner_user_id=state.get("owner_user_id", ""),
                state_snapshot={
                    "task_id": state.get("task_id", ""),
                    "owner_user_id": state.get("owner_user_id", ""),
                    "messages": state.get("messages", []),
                    "intent": state.get("intent", ""),
                    "plan": state.get("plan", {})
                }
            )

        # Check medium-risk tools - for now, also require approval
        # This can be refined later with taint checking
        if tool_name in TaintEngine.MEDIUM_RISK_TOOLS:
            raise ApprovalRequest.from_tool_call(
                tool_name=tool_name,
                tool_args=tool_args,
                tool_call_id=tool_call["id"],
                task_id=state.get("task_id", ""),
                owner_user_id=state.get("owner_user_id", ""),
                state_snapshot={
                    "task_id": state.get("task_id", ""),
                    "owner_user_id": state.get("owner_user_id", ""),
                    "messages": state.get("messages", []),
                    "intent": state.get("intent", ""),
                    "plan": state.get("plan", {})
                }
            )

        try:
            if tool_name == "tool_read_file":
                res = tool_read_file.invoke(tool_args)
            elif tool_name == "tool_write_file":
                res = tool_write_file.invoke(tool_args)
            elif tool_name == "tool_list_directory":
                res = tool_list_directory.invoke(tool_args)
            elif tool_name == "tool_get_current_path":
                res = tool_get_current_path.invoke(tool_args)
            else:
                res = f"Tool {tool_name} not found"
        except Exception as e:
            res = f"Error executing tool {tool_name}: {str(e)}"

        results.append(ToolMessage(
            content=str(res),
            name=tool_name,
            tool_call_id=tool_call["id"]
        ))

    return {"messages": results}

def delegate_node(state: ZhongshuState):
    intent = state.get("intent")
    last_message = state["messages"][-1]

    # Check if we're continuing a previous delegate task (user answered a question)
    pending_delegate_state = state.get("pending_delegate_state")
    if pending_delegate_state and not getattr(last_message, "tool_calls", None):
        # User provided an answer, continue the previous hubu task
        delegate_type = pending_delegate_state.get("type")
        if delegate_type == "hubu":
            hubu_graph = build_hubu_graph()
            # Get the stored state and update with user's answer
            hubu_state = pending_delegate_state.get("state", {}).copy()
            previous_question = pending_delegate_state.get("question", "")

            # Add context about the Q&A exchange so LLM understands what to do
            context_msg = HumanMessage(content=(
                f"用户回答了问题「{previous_question}」\n"
                f"用户的回答：{last_message.content}\n\n"
                f"请继续执行任务，使用工具收集所需信息。"
            ))
            hubu_state["messages"] = [context_msg]
            hubu_state["pending_question"] = None  # Clear the pending question

            # Use stream to get intermediate states
            intermediate_messages = []
            res = None

            for output in hubu_graph.stream(hubu_state, {"recursion_limit": 10}):
                for key, value in output.items():
                    if "messages" in value and value["messages"]:
                        last_msg = value["messages"][-1]

                        # Track tool calls from hubu for intermediate feedback
                        if isinstance(last_msg, AIMessage) and last_msg.tool_calls:
                            for tc in last_msg.tool_calls:
                                # Create an intermediate status message for CLI
                                status_msg = AIMessage(
                                    content="",
                                    additional_kwargs={
                                        "intermediate_status": True,
                                        "agent": "hubu",
                                        "tool_name": tc["name"],
                                        "tool_args": tc["args"]
                                    }
                                )
                                intermediate_messages.append(status_msg)

                        # Track tool results
                        elif isinstance(last_msg, ToolMessage):
                            status_msg = AIMessage(
                                content="",
                                additional_kwargs={
                                    "intermediate_status": True,
                                    "agent": "hubu",
                                    "tool_result": True,
                                    "tool_name": last_msg.name
                                }
                            )
                            intermediate_messages.append(status_msg)

                # Get the final state from the last output
                res = list(output.values())[0] if output else None

            if res is None:
                return {"messages": [AIMessage(content="Hubu执行失败")]}

            # Check if hubu needs more user input
            new_pending_question = res.get("pending_question")
            if new_pending_question:
                hubu_state["messages"] = []
                hubu_state["pending_question"] = new_pending_question
                return {
                    "messages": intermediate_messages + [AIMessage(content=new_pending_question, additional_kwargs={"is_question": True})],
                    "pending_delegate_state": {"type": "hubu", "state": hubu_state}
                }

            # Hubu completed, return result to router for formatting
            final_msg = res.get("messages", [AIMessage(content="无结果")])[-1].content
            return {
                "messages": intermediate_messages + [AIMessage(content=final_msg, additional_kwargs={"is_hubu_result": True})],
                "pending_delegate_state": None
            }

    # Extract plan or goal from tool call if present
    goal_text = ""
    if getattr(last_message, "tool_calls", None):
        for tc in last_message.tool_calls:
            if tc["name"] == "delegate_to_shangshu":
                goal_text = tc["args"].get("plan", "")
            elif tc["name"] == "delegate_to_hubu":
                goal_text = tc["args"].get("research_goal", "")
            elif tc["name"] == "delegate_to_bingbu":
                goal_text = tc["args"].get("code_task", "")

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

        tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
        tool_msg = ToolMessage(content="Delegated successfully", name="delegate_to_shangshu", tool_call_id=tool_call_id)

        return {"messages": [tool_msg, AIMessage(content=final_msg)]}

    elif intent == "QA":
        hubu_graph = build_hubu_graph()

        # Extract context from conversation history
        # Collect previous AI responses that might contain relevant info
        existing_context = {}
        for msg in state.get("messages", []):
            if isinstance(msg, AIMessage) and msg.content:
                # Store AI responses as potential context
                content = msg.content
                if len(content) > 50:  # Only substantial responses
                    existing_context[f"prior_info_{len(existing_context)}"] = content[:500]

        hubu_state = {
            "messages": [HumanMessage(content=goal_text)],
            "task_id": state.get("task_id", ""),
            "goal": goal_text,
            "searched_queries": [],
            "visited_urls": [],
            "context": existing_context,
            "pending_question": None
        }

        # Use stream to get intermediate states
        intermediate_messages = []
        res = None

        for output in hubu_graph.stream(hubu_state, {"recursion_limit": 10}):
            for key, value in output.items():
                if "messages" in value and value["messages"]:
                    last_msg = value["messages"][-1]

                    # Track tool calls from hubu for intermediate feedback
                    if isinstance(last_msg, AIMessage) and last_msg.tool_calls:
                        for tc in last_msg.tool_calls:
                            # Create an intermediate status message for CLI
                            status_msg = AIMessage(
                                content="",
                                additional_kwargs={
                                    "intermediate_status": True,
                                    "agent": "hubu",
                                    "tool_name": tc["name"],
                                    "tool_args": tc["args"]
                                }
                            )
                            intermediate_messages.append(status_msg)

                    # Track tool results
                    elif isinstance(last_msg, ToolMessage):
                        status_msg = AIMessage(
                            content="",
                            additional_kwargs={
                                "intermediate_status": True,
                                "agent": "hubu",
                                "tool_result": True,
                                "tool_name": last_msg.name
                            }
                        )
                        intermediate_messages.append(status_msg)

            # Get the final state from the last output
            res = list(output.values())[0] if output else None

        if res is None:
            return {"messages": [AIMessage(content="Hubu执行失败")]}

        # Check if hubu needs user input
        pending_question = res.get("pending_question")
        if pending_question:
            hubu_state["messages"] = []
            tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
            tool_msg = ToolMessage(content="Need user input", name="delegate_to_hubu", tool_call_id=tool_call_id)
            return {
                "messages": intermediate_messages + [tool_msg],
                "pending_delegate_state": {"type": "hubu", "state": hubu_state, "question": pending_question}
            }

        # Hubu completed, return result to router for formatting
        final_msg = res.get("messages", [AIMessage(content="无结果")])[-1].content
        tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
        tool_msg = ToolMessage(content=final_msg, name="delegate_to_hubu", tool_call_id=tool_call_id)

        return {"messages": intermediate_messages + [tool_msg]}

    elif intent == "Code":
        bingbu_graph = build_bingbu_graph()
        bingbu_state = {
            "messages": [HumanMessage(content=goal_text)],
            "task_id": state.get("task_id", ""),
            "goal": goal_text,
            "files_created": [],
            "files_modified": [],
            "commands_executed": [],
            "errors": []
        }
        res = bingbu_graph.invoke(bingbu_state)
        final_msg = res["messages"][-1].content

        tool_call_id = last_message.tool_calls[0]["id"] if getattr(last_message, "tool_calls", None) else "call_1"
        tool_msg = ToolMessage(content=final_msg, name="delegate_to_bingbu", tool_call_id=tool_call_id)

        return {"messages": [tool_msg, AIMessage(content=final_msg)]}

    return {"messages": []}

def build_zhongshu_graph():
    workflow = StateGraph(ZhongshuState)

    workflow.add_node("router", router_node)
    workflow.add_node("tools", tool_node)
    workflow.add_node("delegate", delegate_node)

    workflow.set_entry_point("router")

    def should_continue(state: ZhongshuState):
        intent = state.get("intent")

        if intent in ["Task", "QA", "Code", "Continue"]:
            return "delegate"

        last_message = state["messages"][-1]
        if getattr(last_message, "tool_calls", None):
            return "tools"

        return END

    def should_continue_after_delegate(state: ZhongshuState):
        last_message = state["messages"][-1] if state["messages"] else None

        # If delegate returned a result to format, go to router
        if isinstance(last_message, AIMessage):
            kwargs = last_message.additional_kwargs
            if kwargs.get("is_hubu_result") or kwargs.get("is_question"):
                return "router"

        # Check for ToolMessage from delegate
        if isinstance(last_message, ToolMessage) and last_message.name == "delegate_to_hubu":
            return "router"

        # Otherwise end
        return END

    workflow.add_conditional_edges("router", should_continue)
    workflow.add_edge("tools", "router")
    workflow.add_conditional_edges("delegate", should_continue_after_delegate)

    return workflow.compile()
