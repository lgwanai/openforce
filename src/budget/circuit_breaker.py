"""Circuit breaker for budget protection.

This module provides circuit breaker functionality to prevent
LLM invocations when budget is exhausted.
"""

from typing import Optional

from .manager import BudgetManager, BudgetExhaustedError


class CircuitBreaker:
    """Manages circuit breaker state for budget protection.

    Use to block LLM invocations when budget is exhausted.

    Attributes:
        budget_manager: The budget manager to protect.
    """

    def __init__(self, budget_manager: BudgetManager):
        """Initialize circuit breaker.

        Args:
            budget_manager: BudgetManager instance to protect.
        """
        self.budget_manager = budget_manager

    def should_block(self) -> bool:
        """Check if execution should be blocked.

        Returns:
            True if budget is exhausted and execution should be blocked.
        """
        return self.budget_manager.is_exhausted()

    def get_exhaustion_reason(self) -> Optional[str]:
        """Get reason for exhaustion, if any.

        Returns:
            Error message if exhausted, None otherwise.
        """
        if not self.should_block():
            return None
        return self.budget_manager.usage.check_exceeded(
            self.budget_manager.limits
        )

    def check_and_raise(self) -> None:
        """Check budget and raise if exhausted.

        Raises:
            BudgetExhaustedError: If budget is exhausted.
        """
        reason = self.get_exhaustion_reason()
        if reason:
            raise BudgetExhaustedError(reason, self.budget_manager.usage)


def check_budget_before_invoke(budget_manager: BudgetManager) -> None:
    """Convenience function to check budget before LLM invocation.

    Args:
        budget_manager: BudgetManager to check.

    Raises:
        BudgetExhaustedError: If budget is exhausted.
    """
    circuit = CircuitBreaker(budget_manager)
    circuit.check_and_raise()
