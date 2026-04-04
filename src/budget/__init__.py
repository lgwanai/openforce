"""Budget management package.

Provides token, time, and cost budget tracking for LLM operations.
"""

from src.budget.manager import (
    BudgetLimits,
    BudgetUsage,
    BudgetExhaustedError,
    BudgetManager,
)
from src.budget.callbacks import BudgetCallbackHandler
from src.budget.trackers import estimate_tokens_from_response, estimate_tokens_from_messages
from src.budget.timeouts import run_with_timeout, invoke_agent_with_budget
from src.budget.circuit_breaker import CircuitBreaker, check_budget_before_invoke
from src.budget.persistence import (
    persist_budget_usage,
    load_budget_from_task,
    create_budget_manager_from_task,
)
from src.budget.isolation import (
    BudgetAllocationStrategy,
    allocate_child_budgets,
)

__all__ = [
    "BudgetLimits",
    "BudgetUsage",
    "BudgetExhaustedError",
    "BudgetManager",
    "BudgetCallbackHandler",
    "estimate_tokens_from_response",
    "estimate_tokens_from_messages",
    "run_with_timeout",
    "invoke_agent_with_budget",
    "CircuitBreaker",
    "check_budget_before_invoke",
    "persist_budget_usage",
    "load_budget_from_task",
    "create_budget_manager_from_task",
    "BudgetAllocationStrategy",
    "allocate_child_budgets",
]
