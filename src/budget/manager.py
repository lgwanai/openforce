"""Budget manager for token, time, and cost tracking.

This module provides the core budget management functionality for tracking
resource consumption in LLM-based agent systems.
"""

from dataclasses import dataclass, field
from typing import Optional, Dict, Any
import asyncio
import time


@dataclass
class BudgetLimits:
    """Budget limits for a task.

    Attributes:
        max_tokens: Maximum number of tokens allowed.
        max_time_seconds: Maximum execution time in seconds.
        max_cost_usd: Maximum cost in USD.
    """

    max_tokens: Optional[int] = None
    max_time_seconds: Optional[int] = None
    max_cost_usd: Optional[float] = None


@dataclass
class BudgetUsage:
    """Current budget consumption.

    Attributes:
        tokens_used: Number of tokens consumed.
        time_elapsed_seconds: Time elapsed since start.
        cost_usd: Cost accumulated in USD.
        start_time: Timestamp when tracking started.
    """

    tokens_used: int = 0
    time_elapsed_seconds: float = 0.0
    cost_usd: float = 0.0
    start_time: float = field(default_factory=time.time)

    def check_exceeded(self, limits: BudgetLimits) -> Optional[str]:
        """Check if any budget limit is exceeded.

        Args:
            limits: The budget limits to check against.

        Returns:
            Error message if a limit is exceeded, None otherwise.
        """
        if limits.max_tokens is not None and self.tokens_used > limits.max_tokens:
            return f"Token budget exceeded: {self.tokens_used}/{limits.max_tokens}"

        if limits.max_time_seconds is not None:
            elapsed = time.time() - self.start_time
            if elapsed > limits.max_time_seconds:
                return f"Time budget exceeded: {elapsed:.1f}s/{limits.max_time_seconds}s"

        if limits.max_cost_usd is not None and self.cost_usd > limits.max_cost_usd:
            return f"Cost budget exceeded: ${self.cost_usd:.4f}/${limits.max_cost_usd}"

        return None


class BudgetExhaustedError(Exception):
    """Raised when budget is exhausted.

    Attributes:
        message: Description of which budget limit was exceeded.
        usage: The current budget usage at time of exhaustion.
    """

    def __init__(self, message: str, usage: BudgetUsage):
        self.message = message
        self.usage = usage
        super().__init__(message)


class BudgetManager:
    """Central budget management for agent tasks.

    Supports hierarchical budget allocation for concurrent agents.
    Thread-safe via asyncio locks for async operations.

    Attributes:
        limits: Budget limits for this manager.
        usage: Current consumption tracking.
        parent: Optional parent manager for hierarchical budgets.
    """

    def __init__(
        self, limits: BudgetLimits, parent: Optional["BudgetManager"] = None
    ):
        """Initialize budget manager.

        Args:
            limits: Budget limits to enforce.
            parent: Optional parent manager for budget hierarchy.
        """
        self.limits = limits
        self.usage = BudgetUsage()
        self.parent = parent
        self._children: Dict[str, "BudgetManager"] = {}
        self._exhausted = False
        self._lock = asyncio.Lock()

    def allocate_child(
        self, agent_id: str, limits: BudgetLimits
    ) -> "BudgetManager":
        """Allocate a child budget for a concurrent agent.

        Args:
            agent_id: Unique identifier for the child agent.
            limits: Budget limits for the child.

        Returns:
            A new BudgetManager for the child agent.
        """
        child = BudgetManager(limits=limits, parent=self)
        self._children[agent_id] = child
        return child

    async def consume_tokens(self, tokens: int) -> None:
        """Record token consumption, checking limits.

        Args:
            tokens: Number of tokens to consume.

        Raises:
            BudgetExhaustedError: If budget is already exhausted or consumption would exceed limits.
        """
        async with self._lock:
            # Check if already exhausted
            if self._exhausted:
                raise BudgetExhaustedError(
                    "Budget already exhausted",
                    self.usage
                )

            self.usage.tokens_used += tokens

            # Propagate to parent if exists
            if self.parent is not None:
                await self.parent.consume_tokens(tokens)

            # Check if we've exceeded limits
            exceeded = self.usage.check_exceeded(self.limits)
            if exceeded:
                self._exhausted = True
                raise BudgetExhaustedError(exceeded, self.usage)

    def is_exhausted(self) -> bool:
        """Check if budget is exhausted (for circuit breaker).

        Returns:
            True if budget is exhausted, False otherwise.
        """
        if self._exhausted:
            return True

        # Check current state against limits
        if self.usage.check_exceeded(self.limits):
            self._exhausted = True

        return self._exhausted
