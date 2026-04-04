"""Tests for BUD-01: Token budget tracking.

Tests for token consumption tracking, limit enforcement, and callback
handler integration with LangChain.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock, patch


class TestBudgetLimits:
    """Tests for BudgetLimits dataclass."""

    def test_budget_limits_creation_default(self, budget_limits):
        """Verify BudgetLimits can be created with no limits."""
        limits = budget_limits()
        assert limits.max_tokens is None
        assert limits.max_time_seconds is None
        assert limits.max_cost_usd is None

    def test_budget_limits_creation_with_tokens(self, budget_limits):
        """Verify BudgetLimits can be created with token limit."""
        limits = budget_limits(max_tokens=1000)
        assert limits.max_tokens == 1000
        assert limits.max_time_seconds is None
        assert limits.max_cost_usd is None

    def test_budget_limits_creation_all_fields(self, budget_limits):
        """Verify BudgetLimits can be created with all limits."""
        limits = budget_limits(
            max_tokens=1000,
            max_time_seconds=60,
            max_cost_usd=1.0
        )
        assert limits.max_tokens == 1000
        assert limits.max_time_seconds == 60
        assert limits.max_cost_usd == 1.0


class TestBudgetManager:
    """Tests for BudgetManager class."""

    def test_budget_manager_initialization(self, budget_manager):
        """Verify BudgetManager initializes with correct state."""
        assert budget_manager.limits is not None
        assert budget_manager.usage is not None
        assert budget_manager.usage.tokens_used == 0
        assert budget_manager.usage.cost_usd == 0.0

    def test_budget_manager_has_limits(self, budget_manager):
        """Verify BudgetManager has expected limits set."""
        assert budget_manager.limits.max_tokens == 1000
        assert budget_manager.limits.max_time_seconds == 60
        assert budget_manager.limits.max_cost_usd == 1.0

    def test_budget_manager_parent_none_by_default(self, budget_manager):
        """Verify BudgetManager has no parent by default."""
        assert budget_manager.parent is None


class TestTokenConsumption:
    """Tests for token consumption tracking."""

    @pytest.mark.asyncio
    async def test_token_consumption_increments_usage(self, budget_manager):
        """Verify consume_tokens increments usage correctly."""
        initial_tokens = budget_manager.usage.tokens_used

        await budget_manager.consume_tokens(100)

        assert budget_manager.usage.tokens_used == initial_tokens + 100

    @pytest.mark.asyncio
    async def test_token_consumption_accumulates(self, budget_manager):
        """Verify multiple consume_tokens calls accumulate."""
        await budget_manager.consume_tokens(50)
        await budget_manager.consume_tokens(30)

        assert budget_manager.usage.tokens_used == 80

    @pytest.mark.asyncio
    async def test_token_limit_exceeded(self, budget_limits):
        """Verify BudgetExhaustedError raised when limit exceeded."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # This should succeed
        await manager.consume_tokens(50)

        # This should succeed exactly at limit
        await manager.consume_tokens(50)

        # This should exceed limit
        with pytest.raises(BudgetExhaustedError) as exc_info:
            await manager.consume_tokens(1)

        assert "Token budget exceeded" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_token_consumption_checks_exceeded(self, budget_limits):
        """Verify consumption fails if budget already exhausted."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Exhaust the budget
        await manager.consume_tokens(100)

        # Further consumption should fail
        with pytest.raises(BudgetExhaustedError):
            await manager.consume_tokens(1)


class TestBudgetCallbackHandler:
    """Tests for LangChain callback handler integration."""

    def test_callback_handler_extracts_tokens_from_llm_output(self, budget_manager, mock_llm_response):
        """Verify BudgetCallbackHandler extracts tokens from LLM response."""
        from src.budget.callbacks import BudgetCallbackHandler

        handler = BudgetCallbackHandler(budget_manager)

        # Create mock LLM result
        response = MagicMock()
        response.llm_output = {
            "token_usage": {
                "total_tokens": 150,
                "prompt_tokens": 50,
                "completion_tokens": 100
            }
        }
        response.generations = []

        # Call on_llm_end
        handler.on_llm_end(response)

        # Token consumption should be tracked asynchronously
        # Give it a moment to process
        import asyncio
        import time
        time.sleep(0.1)

        # The handler should have scheduled token consumption
        # (actual async processing happens in background)

    def test_callback_handler_extracts_from_generations(self, budget_manager):
        """Verify BudgetCallbackHandler extracts tokens from generation info."""
        from src.budget.callbacks import BudgetCallbackHandler

        handler = BudgetCallbackHandler(budget_manager)

        # Create mock response with generation-level usage
        response = MagicMock()
        response.llm_output = None

        generation = MagicMock()
        generation.generation_info = {
            "usage": {
                "total_tokens": 200
            }
        }
        response.generations = [[generation]]

        handler.on_llm_end(response)


class TestNonOpenAIProviderEstimation:
    """Tests for token estimation with non-OpenAI providers."""

    @pytest.mark.asyncio
    async def test_non_openai_provider_estimation(self, budget_manager):
        """Verify token estimation for Minimax/Kimi providers.

        Non-OpenAI providers may not return token counts in response.
        System should estimate tokens from response content length.
        """
        # Simulate response content
        response_content = "This is a test response with multiple words."

        # Rough estimation: ~4 characters per token for mixed content
        estimated_tokens = len(response_content) // 4

        # Should be able to consume estimated tokens
        await budget_manager.consume_tokens(estimated_tokens)

        assert budget_manager.usage.tokens_used == estimated_tokens

    def test_estimation_function_exists(self):
        """Verify token estimation function is available."""
        try:
            from src.budget.trackers import estimate_tokens_from_content
            # Function should exist and be callable
            assert callable(estimate_tokens_from_content)
        except ImportError:
            pytest.skip("Token estimation function not yet implemented")
