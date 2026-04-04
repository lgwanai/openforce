"""Budget isolation utilities for concurrent agents.

This module provides utilities for allocating and isolating budgets
across concurrent child agents.
"""

from enum import Enum
from typing import List, Optional

from .manager import BudgetLimits


class BudgetAllocationStrategy(Enum):
    """Strategy for allocating budgets to child agents."""

    EQUAL = "equal"        # Divide equally among children
    RESERVE = "reserve"    # Keep 20% for parent, divide rest equally
    CUSTOM = "custom"      # User-provided ratios


def allocate_child_budgets(
    parent_limits: BudgetLimits,
    child_count: int,
    strategy: str = "equal",
    custom_ratios: Optional[List[float]] = None
) -> List[BudgetLimits]:
    """Allocate budgets for child agents.

    Args:
        parent_limits: Parent budget limits.
        child_count: Number of child agents.
        strategy: Allocation strategy ("equal", "reserve", "custom").
        custom_ratios: Custom ratios for each child (required if strategy="custom").

    Returns:
        List of BudgetLimits, one per child agent.
    """
    if child_count <= 0:
        return []

    if strategy == "equal":
        return _allocate_equal(parent_limits, child_count)
    elif strategy == "reserve":
        return _allocate_reserve(parent_limits, child_count)
    elif strategy == "custom":
        return _allocate_custom(parent_limits, child_count, custom_ratios)
    else:
        # Default to equal
        return _allocate_equal(parent_limits, child_count)


def _allocate_equal(parent_limits: BudgetLimits, child_count: int) -> List[BudgetLimits]:
    """Divide parent budget equally among children."""
    child_limits = []

    per_child_tokens = None
    if parent_limits.max_tokens is not None:
        per_child_tokens = parent_limits.max_tokens // child_count

    per_child_cost = None
    if parent_limits.max_cost_usd is not None:
        per_child_cost = parent_limits.max_cost_usd / child_count

    for _ in range(child_count):
        child_limits.append(BudgetLimits(
            max_tokens=per_child_tokens,
            max_time_seconds=parent_limits.max_time_seconds,  # Time not divided
            max_cost_usd=per_child_cost,
        ))

    return child_limits


def _allocate_reserve(parent_limits: BudgetLimits, child_count: int) -> List[BudgetLimits]:
    """Keep 20% for parent, divide rest equally among children."""
    reserve_ratio = 0.2

    available_tokens = None
    if parent_limits.max_tokens is not None:
        available_tokens = int(parent_limits.max_tokens * (1 - reserve_ratio))
        per_child_tokens = available_tokens // child_count
    else:
        per_child_tokens = None

    available_cost = None
    if parent_limits.max_cost_usd is not None:
        available_cost = parent_limits.max_cost_usd * (1 - reserve_ratio)
        per_child_cost = available_cost / child_count
    else:
        per_child_cost = None

    child_limits = []
    for _ in range(child_count):
        child_limits.append(BudgetLimits(
            max_tokens=per_child_tokens,
            max_time_seconds=parent_limits.max_time_seconds,
            max_cost_usd=per_child_cost,
        ))

    return child_limits


def _allocate_custom(
    parent_limits: BudgetLimits,
    child_count: int,
    custom_ratios: Optional[List[float]]
) -> List[BudgetLimits]:
    """Allocate based on custom ratios."""
    if not custom_ratios or len(custom_ratios) != child_count:
        # Fall back to equal allocation
        return _allocate_equal(parent_limits, child_count)

    total_ratio = sum(custom_ratios)
    if total_ratio == 0:
        return _allocate_equal(parent_limits, child_count)

    child_limits = []
    for ratio in custom_ratios:
        per_child_tokens = None
        if parent_limits.max_tokens is not None:
            per_child_tokens = int(parent_limits.max_tokens * ratio / total_ratio)

        per_child_cost = None
        if parent_limits.max_cost_usd is not None:
            per_child_cost = parent_limits.max_cost_usd * ratio / total_ratio

        child_limits.append(BudgetLimits(
            max_tokens=per_child_tokens,
            max_time_seconds=parent_limits.max_time_seconds,
            max_cost_usd=per_child_cost,
        ))

    return child_limits
