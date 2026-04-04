import uuid
import sys
from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel
from rich.syntax import Syntax
from rich.prompt import Confirm
import json
from src.core.db import init_db, set_active_task, get_active_task, TaskRecord, save_task
from src.core.logger import log_audit_event
from src.agents.zhongshu import build_zhongshu_graph
from src.tools.base import set_approval_callback, SecurityError
from langchain_core.messages import HumanMessage, AIMessage, ToolMessage

console = Console()

# Session-level context management
_session_approvals = {}
_session_messages = []  # Keep conversation history
_pending_delegate_state = None  # Store delegate state when waiting for user input


def approval_callback(operation: str, path: str, details: dict) -> bool:
    """Handle approval requests from tools."""
    cache_key = f"{operation}:{path}"

    if cache_key in _session_approvals:
        console.print(f"[dim]✓ 使用已批准的权限: {operation} {path}[/dim]")
        return True

    if not sys.stdin.isatty():
        console.print(f"[dim]✓ 非交互模式，自动批准: {operation} {path}[/dim]")
        return True

    console.print()
    console.print(Panel(
        f"[bold yellow]权限请求[/bold yellow]\n\n"
        f"操作类型: [bold]{operation}[/bold]\n"
        f"目标路径: [bold]{path}[/bold]",
        title="[bold red]需要批准[/bold red]",
        border_style="yellow"
    ))

    approved = Confirm.ask("[bold]是否批准此操作?[/bold]", default=True)

    if approved:
        _session_approvals[cache_key] = True
        console.print("[green]✓ 已批准[/green]")
        log_audit_event("approval", "APPROVED", f"{operation}: {path}")
    else:
        console.print("[red]✗ 已拒绝[/red]")
        log_audit_event("approval", "DENIED", f"{operation}: {path}")

    return approved


def run_cli():
    global _session_messages, _pending_delegate_state

    console.print("[bold green]Welcome to OpenForce Agent (CLI Channel)[/bold green]")
    console.print("[dim]提示: Agent 访问项目文件时需要你的批准[/dim]")
    console.print()

    owner_user_id = "local_admin"

    set_approval_callback(approval_callback)

    graph = build_zhongshu_graph()

    while True:
        try:
            user_input = console.input("[bold blue]User > [/bold blue]")
            if not user_input.strip():
                continue

            if user_input.startswith("/"):
                cmd = user_input.strip().split(" ")[0]
                if cmd == "/exit" or cmd == "/quit":
                    break
                elif cmd == "/status":
                    active = get_active_task(owner_user_id)
                    console.print(f"Active task: {active}")
                elif cmd == "/clear-approvals":
                    _session_approvals.clear()
                    console.print("[green]已清除所有已批准的权限[/green]")
                elif cmd == "/clear-history":
                    _session_messages = []
                    _pending_delegate_state = None
                    console.print("[green]已清除对话历史[/green]")
                else:
                    console.print(f"[yellow]Unknown command: {cmd}[/yellow]")
                continue

            # Add user message to history
            _session_messages.append(HumanMessage(content=user_input))

            active_task = get_active_task(owner_user_id)
            if active_task:
                console.print("[yellow]Task is currently running. Please wait or use /stop[/yellow]")
                continue

            task_id = f"task_{uuid.uuid4().hex[:8]}"
            set_active_task(owner_user_id, task_id)

            task = TaskRecord(
                task_id=task_id,
                owner_user_id=owner_user_id,
                conversation_id="conv_1",
                thread_id="thread_1",
                original_req=user_input,
                status="Running"
            )
            save_task(task)

            try:
                log_audit_event(task_id, "USER_INPUT", user_input)
                final_response = None

                # Build initial state with full conversation history
                initial_state = {
                    "messages": _session_messages.copy(),
                    "task_id": task_id,
                    "owner_user_id": owner_user_id,
                    "pending_delegate_state": _pending_delegate_state
                }

                for output in graph.stream(initial_state, {"recursion_limit": 20}):
                    for key, value in output.items():
                        # Check for pending delegate state
                        if "pending_delegate_state" in value:
                            _pending_delegate_state = value["pending_delegate_state"]

                        if "messages" in value and value["messages"]:
                            last_msg = value["messages"][-1]

                            # Add to session history (only new messages)
                            if last_msg not in _session_messages:
                                _session_messages.append(last_msg)

                            msg_type = type(last_msg).__name__
                            msg_content = last_msg.content if hasattr(last_msg, "content") else str(last_msg)

                            if isinstance(last_msg, AIMessage) and last_msg.tool_calls:
                                log_audit_event(task_id, f"AGENT_CALL_TOOL", str(last_msg.tool_calls))
                            else:
                                log_audit_event(task_id, f"AGENT_OUTPUT_{msg_type}", msg_content)

                            if isinstance(last_msg, AIMessage):
                                if last_msg.tool_calls:
                                    console.print()
                                    console.print(Panel(
                                        "[bold yellow]思考中...[/bold yellow]",
                                        title="[bold cyan]System[/bold cyan]",
                                        border_style="dim"
                                    ))
                                    for tc in last_msg.tool_calls:
                                        args_display = json.dumps(tc['args'], ensure_ascii=False, indent=2)
                                        console.print(f"[dim]├── 调用工具: [bold]{tc['name']}[/bold][/dim]")
                                        console.print(f"[dim]└── 参数: {args_display}[/dim]")
                                elif last_msg.content and last_msg.content.strip():
                                    final_response = last_msg.content

                            elif isinstance(last_msg, ToolMessage):
                                console.print(f"[dim]✓ 工具 [bold]{last_msg.name}[/bold] 执行完成[/dim]")

                if final_response:
                    console.print()
                    console.print(Panel(
                        Markdown(final_response),
                        title="[bold green]Assistant[/bold green]",
                        border_style="green"
                    ))

                task.status = "Succeeded"
                save_task(task)

            except SecurityError as e:
                console.print(f"[bold red]安全错误: {e}[/bold red]")
                log_audit_event(task_id, "SECURITY_ERROR", str(e))
            except Exception as e:
                console.print(f"[bold red]Error: {e}[/bold red]")
                log_audit_event(task_id, "ERROR", str(e))
            finally:
                set_active_task(owner_user_id, None)
                console.print()

        except KeyboardInterrupt:
            console.print("\n[yellow]Interrupted by user. Exiting...[/yellow]")
            break
        except Exception as e:
            console.print(f"[bold red]Fatal error: {e}[/bold red]")
            break


if __name__ == "__main__":
    run_cli()
