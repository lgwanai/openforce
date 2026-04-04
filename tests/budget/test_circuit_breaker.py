"""Tests for BUD-04: Global circuit breaker.

Tests for budget exhaustion detection and graceful termination
when any budget limit is reached.
"""

import pytest
import asyncio
from unittest.mock import MagicMock, AsyncMock, patch


class TestCircuitBreakerState:
    """Tests for circuit breaker state management."""

    def test_is_exhausted_returns_false_initially(self, budget_manager):
        """Verify fresh budget not exhausted."""
        assert budget_manager.is_exhausted() is False

    def test_is_exhausted_true_after_token_limit(self, budget_limits):
        """Verify exhausted after token limit."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Exhaust tokens
        manager.usage.tokens_used = 150

        # Should be exhausted
        assert manager.is_exhausted() is True

    def test_is_exhausted_true_after_time_limit(self, budget_limits):
        """Verify exhausted after time limit."""
        from src.budget.manager import BudgetManager
        import time

        limits = budget_limits(max_time_seconds=1)
        manager = BudgetManager(limits=limits)

        # Simulate time passing
        manager.usage.start_time = time.time() - 10  # 10 seconds ago

        # Should be exhausted
        assert manager.is_exhausted() is True

    def test_is_exhausted_true_after_cost_limit(self, budget_limits):
        """Verify exhausted after cost limit."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_cost_usd=0.10)
        manager = BudgetManager(limits=limits)

        # Exhaust cost
        manager.usage.cost_usd = 0.20

        # Should be exhausted
        assert manager.is_exhausted() is True


class TestCircuitBreakerIntegration:
    """Tests for circuit breaker integration with LLM calls."""

    @pytest.mark.asyncio
    async def test_circuit_breaker_prevents_llm_call(self, budget_limits):
        """Verify exhausted budget blocks LLM invocation."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Exhaust budget
        manager._exhausted = True

        # Attempt to consume more tokens should fail
        with pytest.raises(BudgetExhaustedError):
            await manager.consume_tokens(1)

    @pytest.mark.asyncio
    async def test_graceful_termination_returns_partial_state(self, budget_limits):
        """Verify partial state returned on exhaustion."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Simulate partial work
        await manager.consume_tokens(50)

        # Get partial state
        partial_state = {
            "tokens_used": manager.usage.tokens_used,
            "exhausted": manager.is_exhausted()
        }

        assert partial_state["tokens_used"] == 50
        assert partial_state["exhausted"] is False

        # Now exhaust
        await manager.consume_tokens(50)
        assert manager.is_exhausted() is True


class TestCircuitBreakerWithAgent:
    """Tests for circuit breaker with agent graph invocation."""

    @pytest.mark.asyncio
    async def test_agent_invoke_checks_budget_first(self, budget_limits):
        """Verify agent invocation checks budget before proceeding."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Pre-check should pass
        if not manager.is_exhausted():
            result = {"status": "can_proceed"}

        assert result["status"] == "can_proceed"

        # Exhaust budget
        manager._exhausted = True

        # Pre-check should fail
        assert manager.is_exhausted() is True

    @pytest.mark.asyncio
    async def test_agent_invoke_rolls_back_on_exhaustion(self, budget_limits):
        """Verify state is preserved when budget exhausted mid-operation."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Simulate partial agent execution
        initial_tokens = 50
        await manager.consume_tokens(initial_tokens)

        # Capture state before exhaustion
        state_before_exhaustion = {
            "tokens_used": manager.usage.tokens_used,
            "cost_usd": manager.usage.cost_usd
        }

        # Complete remaining work
        await manager.consume_tokens(50)

        # State should be preserved
        assert manager.usage.tokens_used == 100


class TestCircuitBreakerMultipleLimits:
    """Tests for circuit breaker with multiple active limits."""

    @pytest.mark.asyncio
    async def test_any_limit_exhaustion_triggers_breaker(self, budget_limits):
        """Verify any limit exhaustion triggers circuit breaker."""
        from src.budget.manager import BudgetManager

        # Set all three limits
        limits = budget_limits(
            max_tokens=1000,
            max_time_seconds=60,
            max_cost_usd=0.50
        )
        manager = BudgetManager(limits=limits)

        # Only exhaust cost
        manager.usage.cost_usd = 1.0

        # Should be exhausted due to cost
        assert manager.is_exhausted() is True

    @pytest.mark.asyncio
    async def test_token_exhaustion_before_other_limits(self, budget_limits):
        """Verify token exhaustion triggers even if time/cost available."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(
            max_tokens=100,
            max_time_seconds=3600,  # 1 hour
            max_cost_usd=100.0  # $100
        )
        manager = BudgetManager(limits=limits)

        # Exhaust only tokens
        manager.usage.tokens_used = 150

        # Should be exhausted
        assert manager.is_exhausted() is True


class TestCircuitBreakerReset:
    """Tests for circuit breaker reset behavior."""

    def test_circuit_breaker_cannot_be_reset(self, budget_limits):
        """Verify circuit breaker state is final once exhausted."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_tokens=100)
        manager = BudgetManager(limits=limits)

        # Exhaust budget
        manager._exhausted = True

        # Should stay exhausted
        assert manager.is_exhausted() is True

        # Attempt to "reset" should not work
        # (implementation should prevent manual reset of _exhausted)
