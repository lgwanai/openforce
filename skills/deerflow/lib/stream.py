"""Stream event handling and output formatting.

This module provides real-time streaming output for deer-flow agent responses:
- Token-by-token text streaming with flush=True
- Tool call notifications during agent runs
- Tool completion notifications
- LLM retry progress display

The stream_and_print function iterates over DeerFlowClient.stream() events
and formats them for real-time user feedback.
"""
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from deerflow.client import DeerFlowClient


def stream_and_print(
    client: "DeerFlowClient",
    message: str,
    thread_id: str | None = None
) -> str:
    """Stream agent response with real-time output.

    Prints:
    - Token deltas as they arrive (to stdout with flush=True)
    - Tool call notifications: "[Calling: {tool_name}]" (to stderr)
    - Tool completion notifications: "[Tool {name} completed]" (to stderr)
    - LLM retry progress: "[LLM retry {attempt}/{max}, waiting {wait}s]" (to stderr)

    Tool execution errors are handled gracefully (ERRR-02):
    - Prints warning to stderr: "[Tool {name} error: {message}]"
    - Continues processing subsequent events
    - Does NOT raise exception for tool errors

    Error propagation behavior:
    - GeneratorExit: Propagates naturally (user interrupt via Ctrl+C)
    - Other exceptions: Propagate to caller (handled by skill.py)
    - Partial output: Preserved in stdout before exception

    Note: This function does NOT wrap the stream loop in try/except.
    Exceptions propagate naturally for proper handling by the caller.
    The deerflow-harness ToolErrorHandlingMiddleware handles tool errors
    internally and emits error ToolMessages, which we detect and format.

    Args:
        client: DeerFlowClient instance with stream() method.
        message: User prompt to send to the agent.
        thread_id: Optional thread ID for conversation continuity.

    Returns:
        The final accumulated text response from the AI.

    Raises:
        GeneratorExit: If the stream is interrupted (user interrupt).
        Exception: Other exceptions propagate to the caller.
    """
    chunks: dict[str, list[str]] = {}
    last_id: str = ""

    for event in client.stream(message, thread_id=thread_id):
        if event.type == "messages-tuple":
            data = event.data

            if data.get("type") == "ai":
                msg_id = data.get("id") or ""
                content = data.get("content", "")

                # Print and accumulate token delta
                if content:
                    print(content, end="", flush=True)
                    chunks.setdefault(msg_id, []).append(content)
                    last_id = msg_id

                # Tool call notification
                if data.get("tool_calls"):
                    for tc in data["tool_calls"]:
                        print(
                            f"\n[Calling: {tc['name']}]",
                            file=sys.stderr,
                            flush=True
                        )

            elif data.get("type") == "tool":
                tool_name = data.get("name", "tool")

                # Check for tool error
                if data.get("error"):
                    error_content = data.get("content", "Unknown error")
                    print(
                        f"\n[Tool {tool_name} error: {error_content}]",
                        file=sys.stderr,
                        flush=True
                    )
                else:
                    print(
                        f"\n[Tool {tool_name} completed]",
                        file=sys.stderr,
                        flush=True
                    )

        elif event.type == "custom":
            # Handle middleware custom events (e.g., llm_retry)
            data = event.data
            if data.get("type") == "llm_retry" or "llm_retry" in str(data):
                # Handle both {type: "llm_retry"} and {event: "retry"} formats
                if data.get("type") == "llm_retry":
                    attempt = data.get("attempt", 0)
                    max_attempts = data.get("max_attempts", 3)
                    wait_ms = data.get("wait_ms", 0)
                else:
                    # Alternative format from retry_stream fixture
                    attempt = data.get("attempt", 0)
                    max_attempts = data.get("max_attempts", 3)
                    wait_ms = data.get("wait_ms", 1000)

                wait_s = wait_ms / 1000
                print(
                    f"\n[LLM retry {attempt}/{max_attempts}, waiting {wait_s:.0f}s]",
                    file=sys.stderr,
                    flush=True
                )

        elif event.type == "end":
            # Stream complete - usage stats available in event.data
            pass

    # Return accumulated content from the last message ID
    return "".join(chunks.get(last_id, []))
