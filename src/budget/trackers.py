"""Token tracking utilities for budget management.

This module provides utilities for estimating token counts when
providers don't return token usage in their API responses.
"""


def estimate_tokens_from_response(response_content: str) -> int:
    """Estimate token count from response content.

    Used for non-OpenAI providers (Minimax, Kimi) that don't return
    token counts in their API responses.

    For Chinese/English mixed text, approximately 4 characters per token
    is a reasonable approximation.

    Args:
        response_content: The text content to estimate tokens for.

    Returns:
        Estimated number of tokens.
    """
    if not response_content:
        return 0

    # ~4 chars per token for Chinese/English mix
    # This is an approximation but works reasonably well
    return len(response_content) // 4


def estimate_tokens_from_messages(messages: list) -> int:
    """Estimate token count from a list of chat messages.

    Args:
        messages: List of message dictionaries with 'role' and 'content' keys.

    Returns:
        Estimated number of tokens.
    """
    total_chars = 0
    for message in messages:
        if isinstance(message, dict):
            content = message.get("content", "")
            role = message.get("role", "")
            total_chars += len(str(content)) + len(role)
        else:
            total_chars += len(str(message))

    return total_chars // 4
