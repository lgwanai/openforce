"""Error message formatting and templates.

This module provides actionable error messages for common failure modes:
- Missing deerflow-harness package
- Missing or invalid config.yaml
- Missing credentials (API keys)
- Streaming errors (recursion limit, LLM timeout, quota, auth)

All messages are designed to guide users toward resolution with specific
commands and examples.
"""

# Streaming-specific error templates
STREAMING_ERRORS = {
    "recursion": """The agent reached its maximum reasoning steps (recursion limit).

This usually happens when:
- The task is very complex and requires many tool calls
- The agent is stuck in a loop

What to try:
- Use --pro mode for better planning on complex tasks
- Simplify your request into smaller subtasks
- Check if your query has an answer (some questions have no solution)

Your session context has been preserved. You can continue with a simpler query.""",

    "llm_timeout": """The LLM provider is taking too long to respond.

This can happen when:
- The model is processing a complex request
- The provider is experiencing high load

What to try:
- Wait a moment and try again
- Use a simpler prompt
- Use --flash mode for faster responses""",

    "llm_quota": """The LLM provider quota has been exceeded.

Please check your provider account:
- OpenAI: https://platform.openai.com/account/usage
- Anthropic: https://console.anthropic.com/settings/usage

What to try:
- Update your billing information
- Wait for quota reset (often monthly)
- Use a different model provider""",

    "llm_auth": """Authentication failed with the LLM provider.

Please check your API key:
- Ensure OPENAI_API_KEY or ANTHROPIC_API_KEY is set correctly
- Verify the key has not expired or been revoked
- Check that the key has appropriate permissions

To update your API key:
- Export in shell: export OPENAI_API_KEY=sk-...
- Or update your config.yaml with the new key""",
}

# Session behavior documentation
STATELESS_SESSION_INFO = """Note: Each skill invocation is stateless by default.

- Previous conversation turns are NOT remembered
- Tool results from previous calls are NOT available
- Each call starts fresh with your prompt

The thread_id is used only for file isolation (preventing concurrent
writes), not for conversation continuity.

For multi-turn conversations with persistent memory, use the deer-flow
server which provides checkpointer-backed thread storage."""

# Template for missing deerflow-harness package
MISSING_PACKAGE_MSG = """deerflow-harness is not installed. Install with:

    pip install deerflow-harness

or:

    uv add deerflow-harness

For local development, you can also install from workspace:

    pip install -e /path/to/deer-flow/backend/packages/harness
"""

# Template for missing config.yaml
MISSING_CONFIG_MSG = """config.yaml not found.

I've created config.example.yaml with a template configuration.
Copy it to config.yaml and configure your models:

    cp config.example.yaml config.yaml

Then edit config.yaml to add your API keys.
"""

# Template for missing credentials
MISSING_CREDENTIALS_MSG = """Missing required credentials.

The following environment variables need to be set:
  - OPENAI_API_KEY - Get from https://platform.openai.com/api-keys
  - ANTHROPIC_API_KEY - Get from https://console.anthropic.com/settings/keys

Example config.yaml:
  models:
    - name: gpt-4
      use: langchain_openai:ChatOpenAI
      api_key: "$OPENAI_API_KEY"

    - name: claude-3-sonnet
      use: langchain_anthropic:ChatAnthropic
      api_key: "$ANTHROPIC_API_KEY"

Or set in shell:
  export OPENAI_API_KEY=sk-...
  export ANTHROPIC_API_KEY=sk-ant-...

Tip: Add these to your ~/.zshrc or ~/.bashrc for persistence.
"""


def format_error(e: Exception) -> str:
    """Format an exception into an actionable error message.

    Matches error types to appropriate templates and extracts
    relevant context for guidance.

    Args:
        e: The exception to format.

    Returns:
        A user-friendly error message with actionable guidance.
    """
    error_type = type(e).__name__
    error_msg = str(e).lower()

    # Check for deerflow-harness import errors
    if isinstance(e, (ImportError, ModuleNotFoundError)):
        if "deerflow" in error_msg:
            return MISSING_PACKAGE_MSG
        # Other import errors - show generic message with hint
        return f"Import error: {e}\n\nMake sure all required packages are installed."

    # Check for config-related file not found errors
    if isinstance(e, FileNotFoundError):
        msg = str(e)
        if "config" in msg.lower():
            return MISSING_CONFIG_MSG
        # Generic file not found
        return f"File not found: {e}"

    # Check for credential-related value errors
    if isinstance(e, ValueError):
        msg = str(e)
        if "credential" in msg.lower() or "api_key" in msg.lower():
            # The error message from validate_config already has detailed guidance
            # Pass it through with a prefix
            return f"{msg}\n\nFor more help, see config.example.yaml"
        if "parse" in msg.lower() or "yaml" in msg.lower():
            # Config parsing errors
            return msg
        # Generic value error
        return f"Configuration error: {e}"

    # For all other errors, return the raw message
    # LLM provider errors should be passed through without wrapping
    return str(e)


def format_streaming_error(e: Exception) -> str:
    """Format streaming-specific errors with actionable guidance.

    Handles errors that occur during agent streaming:
    - GraphRecursionError: Recursion limit exceeded
    - Timeout errors: LLM provider timeout
    - Quota errors: Rate limit or quota exceeded
    - Auth errors: API key authentication failure

    Args:
        e: The exception to format.

    Returns:
        A user-friendly error message with actionable guidance.
    """
    import re

    error_type = type(e).__name__
    error_msg = str(e).lower()

    # Check for GraphRecursionError (from langgraph.errors)
    # Check by name to avoid import dependency
    if error_type == "GraphRecursionError" or "recursion" in error_msg:
        return STREAMING_ERRORS["recursion"]

    # Check for timeout-related errors
    if (
        "timeout" in error_msg or
        "timed out" in error_msg or
        error_type == "TimeoutError" or
        "TimeoutError" in error_type
    ):
        return STREAMING_ERRORS["llm_timeout"]

    # Check for quota/rate limit errors
    if (
        "quota" in error_msg or
        "rate limit" in error_msg or
        "insufficient" in error_msg or
        "exceeded" in error_msg and "limit" in error_msg
    ):
        return STREAMING_ERRORS["llm_quota"]

    # Check for authentication errors
    if (
        "auth" in error_msg or
        "unauthorized" in error_msg or
        "401" in error_msg or
        "invalid api key" in error_msg or
        "api key" in error_msg and ("invalid" in error_msg or "expired" in error_msg)
    ):
        return STREAMING_ERRORS["llm_auth"]

    # Fallback: return the raw error message
    # LLMErrorHandlingMiddleware already formats most errors into AIMessages
    # that are yielded as stream events. This catches edge cases.
    return f"Agent error: {e}"
