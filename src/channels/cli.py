import uuid
from rich.console import Console
from rich.markdown import Markdown
from src.core.db import init_db, set_active_task, get_active_task, TaskRecord, save_task
from src.core.logger import log_audit_event
from src.agents.zhongshu import build_zhongshu_graph
from langchain_core.messages import HumanMessage, AIMessage, ToolMessage

console = Console()

def run_cli():
    console.print("[bold green]Welcome to OpenClaw Agent (CLI Channel)[/bold green]")
    owner_user_id = "local_admin"
    graph = build_zhongshu_graph()
    
    while True:
        try:
            user_input = console.input("[bold blue]User > [/bold blue]")
            if not user_input.strip():
                continue
                
            if user_input.startswith("/"):
                # Handle slash commands
                cmd = user_input.strip().split(" ")[0]
                if cmd == "/exit" or cmd == "/quit":
                    break
                elif cmd == "/status":
                    active = get_active_task(owner_user_id)
                    console.print(f"Active task: {active}")
                else:
                    console.print(f"[yellow]Unknown command: {cmd}[/yellow]")
                continue

            active_task = get_active_task(owner_user_id)
            if active_task:
                console.print("[yellow]Task is currently running. Please wait or use /stop[/yellow]")
                continue
                
            # Create a new session task
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
            
            # Invoke Zhongshu
            initial_state = {
                "messages": [HumanMessage(content=user_input)],
                "task_id": task_id,
                "owner_user_id": owner_user_id
            }
            
            try:
                log_audit_event(task_id, "USER_INPUT", user_input)
                for output in graph.stream(initial_state, {"recursion_limit": 20}):
                    for key, value in output.items():
                        if "messages" in value:
                            last_msg = value["messages"][-1]
                            
                            # Log the interaction
                            msg_type = type(last_msg).__name__
                            msg_content = last_msg.content if hasattr(last_msg, "content") else str(last_msg)
                            
                            if isinstance(last_msg, AIMessage) and last_msg.tool_calls:
                                log_audit_event(task_id, f"AGENT_CALL_TOOL", str(last_msg.tool_calls))
                            else:
                                log_audit_event(task_id, f"AGENT_OUTPUT_{msg_type}", msg_content)
                            
                            # Print to console
                            if isinstance(last_msg, AIMessage):
                                if last_msg.content:
                                    console.print(Markdown(last_msg.content))
                                if last_msg.tool_calls:
                                    for tc in last_msg.tool_calls:
                                        console.print(f"[dim]Agent is calling tool: {tc['name']} with args: {tc['args']}[/dim]")
                            elif isinstance(last_msg, ToolMessage):
                                console.print(f"[dim]Tool {last_msg.name} returned: {last_msg.content}[/dim]")
                                
                    # If the stream finishes and intent is "Chat", we stop.
                    # But actually graph.stream will yield until it reaches END.
                    
                    # Update task state
                    task.status = "Succeeded"
                    save_task(task)
                    
            except Exception as e:
                console.print(f"[bold red]Error: {e}[/bold red]")
                log_audit_event(task_id, "ERROR", str(e))
            finally:
                # Clear active task
                set_active_task(owner_user_id, None)
                
        except KeyboardInterrupt:
            console.print("\n[yellow]Interrupted by user. Exiting...[/yellow]")
            break
        except Exception as e:
            console.print(f"[bold red]Fatal error: {e}[/bold red]")
            break

if __name__ == "__main__":
    run_cli()
