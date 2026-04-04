"""Budget state persistence utilities.

This module provides utilities for persisting and loading budget state
from TaskRecord for recovery and monitoring.
"""

import time
from typing import Tuple, Optional

from ..core.db import TaskRecord, get_task, save_task
from .manager import BudgetLimits, BudgetUsage, BudgetManager


async def persist_budget_usage(task_id: str, budget_manager: BudgetManager) -> None:
    """Persist budget usage to TaskRecord.

    Updates TaskRecord.budget field with current usage and limits.

    Args:
        task_id: ID of the task to update.
        budget_manager: BudgetManager with current state.
    """
    task = get_task(task_id)
    if not task:
        return

    task.budget.update({
        "limits": {
            "max_tokens": budget_manager.limits.max_tokens,
            "max_time_seconds": budget_manager.limits.max_time_seconds,
            "max_cost_usd": budget_manager.limits.max_cost_usd,
        },
        "usage": {
            "tokens_used": budget_manager.usage.tokens_used,
            "time_elapsed_seconds": time.time() - budget_manager.usage.start_time,
            "cost_usd": budget_manager.usage.cost_usd,
        },
        "exhausted": budget_manager.is_exhausted(),
    })

    save_task(task)


def load_budget_from_task(task: TaskRecord) -> Tuple[BudgetLimits, BudgetUsage, bool]:
    """Load budget state from TaskRecord.

    Args:
        task: TaskRecord with budget field.

    Returns:
        Tuple of (BudgetLimits, BudgetUsage, exhausted).
    """
    budget_data = task.budget or {}

    limits_data = budget_data.get("limits", {})
    limits = BudgetLimits(
        max_tokens=limits_data.get("max_tokens"),
        max_time_seconds=limits_data.get("max_time_seconds"),
        max_cost_usd=limits_data.get("max_cost_usd"),
    )

    usage_data = budget_data.get("usage", {})
    usage = BudgetUsage(
        tokens_used=usage_data.get("tokens_used", 0),
        time_elapsed_seconds=usage_data.get("time_elapsed_seconds", 0.0),
        cost_usd=usage_data.get("cost_usd", 0.0),
        start_time=time.time(),  # Reset start time on load
    )

    exhausted = budget_data.get("exhausted", False)

    return limits, usage, exhausted


def create_budget_manager_from_task(task: TaskRecord) -> BudgetManager:
    """Create a BudgetManager from TaskRecord.

    Useful for resuming a task with existing budget state.

    Args:
        task: TaskRecord with budget field.

    Returns:
        BudgetManager with loaded state.
    """
    limits, usage, exhausted = load_budget_from_task(task)

    manager = BudgetManager(limits)
    manager.usage = usage
    manager._exhausted = exhausted

    return manager
