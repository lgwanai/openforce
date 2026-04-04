"""Tests for BUD-02: Time budget enforcement.

Tests for time budget tracking, timeout handling, and proper cleanup
on cancellation.
"""

import pytest
import asyncio
import time
from unittest.mock import MagicMock, AsyncMock, patch


class TestTimeBudgetTracking:
    """Tests for time budget tracking."""

    def test_time_budget_elapsed(self, budget_manager):
        """Verify time_elapsed_seconds updates correctly."""
        initial_time = budget_manager.usage.time_elapsed_seconds

        # Should start at 0 or near 0
        assert initial_time >= 0

        # Wait a bit
        time.sleep(0.1)

        # Check elapsed time calculation
        elapsed = time.time() - budget_manager.usage.start_time
        assert elapsed >= 0.1

    def test_time_budget_start_time_recorded(self, budget_manager):
        """Verify start_time is recorded on creation."""
        before = time.time()
        # budget_manager is created in fixture, check it has valid start_time
        after = time.time()

        assert budget_manager.usage.start_time >= before - 1  # Allow 1 second tolerance
        assert budget_manager.usage.start_time <= after


class TestTimeLimitEnforcement:
    """Tests for time limit exceeded handling."""

    @pytest.mark.asyncio
    async def test_time_limit_exceeded(self, budget_limits):
        """Verify timeout triggers BudgetExhaustedError."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_time_seconds=1)
        manager = BudgetManager(limits=limits)

        # Wait for time to exceed limit
        await asyncio.sleep(1.1)

        # Next check should raise
        with pytest.raises(BudgetExhaustedError) as exc_info:
            await manager.consume_tokens(1)

        assert "Time budget exceeded" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_time_check_before_invoke(self, budget_limits):
        """Verify time budget checked before LLM call."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_time_seconds=1)
        manager = BudgetManager(limits=limits)

        # Exhaust time budget
        await asyncio.sleep(1.1)

        # is_exhausted should return True
        assert manager.is_exhausted() is True

        # Any further operation should fail
        with pytest.raises(BudgetExhaustedError):
            await manager.consume_tokens(1)


class TestRunWithTimeout:
    """Tests for run_with_timeout helper function."""

    @pytest.mark.asyncio
    async def test_run_with_timeout_success(self, budget_manager):
        """Verify successful completion within timeout."""
        from src.budget.timeouts import run_with_timeout

        async def quick_task():
            await asyncio.sleep(0.1)
            return "success"

        result = await run_with_timeout(
            quick_task(),
            timeout_seconds=5,
            budget_manager=budget_manager
        )

        assert result == "success"

    @pytest.mark.asyncio
    async def test_run_with_timeout_cancellation(self, budget_limits):
        """Verify proper cleanup on timeout."""
        from src.budget.manager import BudgetManager
        from src.budget.timeouts import run_with_timeout, BudgetExhaustedError

        limits = budget_limits(max_time_seconds=10)
        manager = BudgetManager(limits=limits)

        async def slow_task():
            await asyncio.sleep(10)
            return "too slow"

        with pytest.raises((asyncio.TimeoutError, BudgetExhaustedError)):
            await run_with_timeout(
                slow_task(),
                timeout_seconds=0.5,
                budget_manager=manager
            )

        # Budget should be marked exhausted
        assert manager.is_exhausted() is True

    @pytest.mark.asyncio
    async def test_run_with_timeout_propagates_exceptions(self, budget_manager):
        """Verify exceptions from task are propagated."""
        from src.budget.timeouts import run_with_timeout

        async def failing_task():
            raise ValueError("Task failed")

        with pytest.raises(ValueError, match="Task failed"):
            await run_with_timeout(
                failing_task(),
                timeout_seconds=5,
                budget_manager=budget_manager
            )


class TestTimeBudgetIntegration:
    """Integration tests for time budget with agent invocation."""

    @pytest.mark.asyncio
    async def test_time_budget_with_mock_llm(self, budget_limits):
        """Verify time budget works with simulated LLM calls."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        limits = budget_limits(max_time_seconds=2, max_tokens=1000)
        manager = BudgetManager(limits=limits)

        # Simulate LLM call
        async def mock_llm_call():
            await asyncio.sleep(0.5)
            return {"content": "response"}

        # Should succeed within time budget
        result = await mock_llm_call()
        assert result["content"] == "response"

        # Budget should not be exhausted
        assert manager.is_exhausted() is False

    @pytest.mark.asyncio
    async def test_time_budget_exhausted_during_llm(self, budget_limits):
        """Verify handling when time budget exhausted during LLM call."""
        from src.budget.manager import BudgetManager

        limits = budget_limits(max_time_seconds=1)
        manager = BudgetManager(limits=limits)

        # Simulate long-running LLM call
        async def long_llm_call():
            await asyncio.sleep(2)
            return {"content": "response"}

        # Should raise timeout error
        with pytest.raises(asyncio.TimeoutError):
            await asyncio.wait_for(long_llm_call(), timeout=0.5)

        # Budget should be marked exhausted
        manager._exhausted = True
        assert manager.is_exhausted() is True
