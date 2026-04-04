"""LangChain callback handler for budget tracking.

This module provides a callback handler that automatically extracts
token usage from LLM responses and consumes budget.
"""

import asyncio
from typing import Any, Dict, List, Optional
from langchain_core.callbacks import BaseCallbackHandler
from langchain_core.outputs import LLMResult

from .manager import BudgetManager


class BudgetCallbackHandler(BaseCallbackHandler):
    """LangChain callback to track token usage for budget management.

    Works with OpenAI-compatible APIs that return token counts in response_metadata.
    For non-standard providers (Minimax, Kimi), use estimate_tokens_from_response.

    Attributes:
        budget_manager: The budget manager to update with token consumption.
    """

    def __init__(self, budget_manager: BudgetManager):
        """Initialize the callback handler.

        Args:
            budget_manager: BudgetManager instance to update with token consumption.
        """
        self.budget_manager = budget_manager
        self._pending_tokens: int = 0  # Track tokens for sync contexts

    def on_llm_end(self, response: LLMResult, **kwargs: Any) -> None:
        """Extract token usage from LLM response and consume budget.

        Args:
            response: The LLM response containing generation results.
            **kwargs: Additional callback arguments.
        """
        total_tokens = 0

        # Try to get tokens from llm_output (standard OpenAI format)
        if response.llm_output:
            token_usage = response.llm_output.get("token_usage", {})
            total_tokens = token_usage.get("total_tokens", 0)

        # Also check individual generations for token info
        if total_tokens == 0 and response.generations:
            for generation_list in response.generations:
                for generation in generation_list:
                    if hasattr(generation, "generation_info") and generation.generation_info:
                        usage = generation.generation_info.get("usage", {})
                        total_tokens = usage.get("total_tokens", 0)
                        if total_tokens:
                            break
                if total_tokens:
                    break

        # Consume tokens if found
        if total_tokens > 0:
            self._pending_tokens = total_tokens
            # Try async first, fall back to sync tracking
            try:
                loop = asyncio.get_running_loop()
                asyncio.create_task(self.budget_manager.consume_tokens(total_tokens))
            except RuntimeError:
                # No running loop - tokens will be consumed when budget is checked
                # or when explicit flush is called in async context
                pass

    def get_pending_tokens(self) -> int:
        """Get the number of pending tokens to be consumed.

        Returns:
            Number of tokens extracted but not yet consumed asynchronously.
        """
        return self._pending_tokens

    async def flush(self) -> None:
        """Flush pending tokens to the budget manager.

        Call this after on_llm_end in async contexts to ensure
        tokens are consumed.
        """
        if self._pending_tokens > 0:
            await self.budget_manager.consume_tokens(self._pending_tokens)
            self._pending_tokens = 0
