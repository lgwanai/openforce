"""Timeout enforcement utilities for budget management.

This module provides utilities for enforcing time budgets on async operations.
"""

import asyncio
from typing import TypeVar, Callable, Any, Coroutine

from .manager import BudgetManager, BudgetExhaustedError

T = TypeVar("T")


async def run_with_timeout(
    coro: Coroutine[Any, Any, T],
    timeout_seconds: int,
    budget_manager: BudgetManager
) -> T:
    """Run a coroutine with timeout and budget check.

    Args:
        coro: The coroutine to run.
        timeout_seconds: Maximum time to allow for execution.
        budget_manager: Budget manager to update on exhaustion.

    Returns:
        The result of the coroutine.

    Raises:
        asyncio.TimeoutError: If timeout exceeded.
        BudgetExhaustedError: If budget exhausted during execution.
    """
    try:
        return await asyncio.wait_for(coro, timeout=timeout_seconds)
    except asyncio.TimeoutError:
        budget_manager._exhausted = True
        raise BudgetExhaustedError(
            f"Time budget exceeded: timeout after {timeout_seconds}s",
            budget_manager.usage
        )
    except asyncio.CancelledError:
        # Proper cleanup on cancellation
        budget_manager._exhausted = True
        raise BudgetExhaustedError(
            "Task cancelled due to budget exhaustion",
            budget_manager.usage
        )


async def invoke_agent_with_budget(
    graph: Any,
    state: Any,
    budget_manager: BudgetManager
) -> Any:
    """Invoke agent graph with budget protection.

    Args:
        graph: The LangGraph or similar graph to invoke.
        state: The state to pass to the graph.
        budget_manager: Budget manager for tracking.

    Returns:
        The result from the graph invocation.

    Raises:
        BudgetExhaustedError: If budget is exhausted before or during execution.
    """
    if budget_manager.is_exhausted():
        raise BudgetExhaustedError(
            "Budget already exhausted",
            budget_manager.usage
        )

    timeout = budget_manager.limits.max_time_seconds or 300

    try:
        # Use ainvoke for async support
        return await run_with_timeout(
            graph.ainvoke(state),
            timeout,
            budget_manager
        )
    except (asyncio.TimeoutError, BudgetExhaustedError):
        # Graceful termination - return partial state
        return {"error": "Budget exhausted", "partial_state": state}
