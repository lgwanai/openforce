"""Tests for BUD-05: Concurrent agent budget isolation.

Tests for hierarchical budget allocation, child agent isolation,
and prevention of budget starvation.
"""

import pytest
import asyncio
from unittest.mock import MagicMock, AsyncMock


class TestChildBudgetAllocation:
    """Tests for child budget creation and isolation."""

    def test_allocate_child_creates_independent_budget(self, budget_limits):
        """Verify child budget has own limits."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        # Allocate child with different limits
        child_limits = budget_limits(max_tokens=100)
        child = parent.allocate_child("agent-1", child_limits)

        # Child should have its own limits
        assert child.limits.max_tokens == 100
        assert child.limits.max_tokens != parent.limits.max_tokens

    def test_child_has_parent_reference(self, budget_limits):
        """Verify child has reference to parent."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child_limits = budget_limits(max_tokens=100)
        child = parent.allocate_child("agent-1", child_limits)

        assert child.parent is parent

    def test_parent_tracks_children(self, budget_limits):
        """Verify parent tracks all children."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child1 = parent.allocate_child("agent-1", budget_limits(max_tokens=100))
        child2 = parent.allocate_child("agent-2", budget_limits(max_tokens=100))

        # Parent should track children
        assert len(parent._children) == 2
        assert "agent-1" in parent._children
        assert "agent-2" in parent._children


class TestChildConsumptionPropagation:
    """Tests for consumption propagation from child to parent."""

    @pytest.mark.asyncio
    async def test_child_consumption_propagates_to_parent(self, budget_limits):
        """Verify parent tracks total consumption."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child_limits = budget_limits(max_tokens=100)
        child = parent.allocate_child("agent-1", child_limits)

        # Child consumes tokens
        await child.consume_tokens(50)

        # Parent should also track this consumption
        assert parent.usage.tokens_used == 50
        assert child.usage.tokens_used == 50

    @pytest.mark.asyncio
    async def test_multiple_children_propagate_to_parent(self, budget_limits):
        """Verify parent tracks total from multiple children."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child1 = parent.allocate_child("agent-1", budget_limits(max_tokens=100))
        child2 = parent.allocate_child("agent-2", budget_limits(max_tokens=100))

        # Both children consume
        await child1.consume_tokens(30)
        await child2.consume_tokens(40)

        # Parent should have total
        assert parent.usage.tokens_used == 70


class TestChildIsolation:
    """Tests for child budget isolation from siblings."""

    @pytest.mark.asyncio
    async def test_child_exhaustion_doesnt_affect_siblings(self, budget_limits):
        """Verify isolation between children."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child1 = parent.allocate_child("agent-1", budget_limits(max_tokens=50))
        child2 = parent.allocate_child("agent-2", budget_limits(max_tokens=100))

        # Child1 exhausts its budget
        await child1.consume_tokens(50)

        # Child1 should be exhausted
        assert child1.is_exhausted() is True

        # Child2 should still have budget
        assert child2.is_exhausted() is False

        # Child2 can still consume
        await child2.consume_tokens(50)
        assert child2.usage.tokens_used == 50

    @pytest.mark.asyncio
    async def test_child_cannot_exceed_own_limit_even_with_parent_budget(self, budget_limits):
        """Verify child respects own limits, not parent limits."""
        from src.budget.manager import BudgetManager, BudgetExhaustedError

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        # Child has limited budget
        child_limits = budget_limits(max_tokens=50)
        child = parent.allocate_child("agent-1", child_limits)

        # Try to consume more than child limit
        await child.consume_tokens(50)

        # Should be exhausted
        assert child.is_exhausted() is True

        # Further consumption should fail
        with pytest.raises(BudgetExhaustedError):
            await child.consume_tokens(1)


class TestAllocationStrategies:
    """Tests for budget allocation strategies."""

    def test_equal_allocation_strategy(self, budget_limits):
        """Verify divide equally strategy."""
        from src.budget.manager import allocate_child_budgets

        parent_limits = budget_limits(max_tokens=1000, max_cost_usd=10.0)
        child_limits = allocate_child_budgets(
            parent_limits,
            child_count=4,
            strategy="equal"
        )

        assert len(child_limits) == 4

        # Each child should get 1/4 of tokens
        for limits in child_limits:
            assert limits.max_tokens == 250

        # Each child should get 1/4 of cost
        for limits in child_limits:
            assert limits.max_cost_usd == 2.5

    def test_reserve_allocation_strategy(self, budget_limits):
        """Verify 20% reserve strategy."""
        from src.budget.manager import allocate_child_budgets

        parent_limits = budget_limits(max_tokens=1000, max_cost_usd=10.0)
        child_limits = allocate_child_budgets(
            parent_limits,
            child_count=4,
            strategy="reserve"
        )

        assert len(child_limits) == 4

        # Each child should get 1/4 of 80% = 200 tokens
        for limits in child_limits:
            assert limits.max_tokens == 200

        # Each child should get 1/4 of 80% = $2.0
        for limits in child_limits:
            assert limits.max_cost_usd == 2.0


class TestNoStarvation:
    """Tests for preventing child agent starvation."""

    @pytest.mark.asyncio
    async def test_no_starvation(self, budget_limits):
        """Verify all children get budget allocation."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        children = []
        for i in range(5):
            child = parent.allocate_child(
                f"agent-{i}",
                budget_limits(max_tokens=200)
            )
            children.append(child)

        # All children should have budget
        for child in children:
            assert child.limits.max_tokens > 0
            assert child.is_exhausted() is False

    @pytest.mark.asyncio
    async def test_fair_distribution_among_children(self, budget_limits):
        """Verify fair distribution when children compete."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=100)
        parent = BudgetManager(limits=parent_limits)

        children = [
            parent.allocate_child(f"agent-{i}", budget_limits(max_tokens=50))
            for i in range(3)
        ]

        # Each child consumes some
        for child in children:
            try:
                await child.consume_tokens(30)
            except Exception:
                pass

        # Parent should track all consumption
        # Total should be 90 (3 * 30)
        assert parent.usage.tokens_used == 90


class TestConcurrentAccess:
    """Tests for concurrent budget access."""

    @pytest.mark.asyncio
    async def test_concurrent_consumption_is_thread_safe(self, budget_limits):
        """Verify concurrent consumption is handled safely."""
        from src.budget.manager import BudgetManager

        parent_limits = budget_limits(max_tokens=1000)
        parent = BudgetManager(limits=parent_limits)

        child = parent.allocate_child("agent-1", budget_limits(max_tokens=500))

        # Simulate concurrent consumption
        async def consume(n):
            for _ in range(n):
                await child.consume_tokens(1)

        # Run multiple concurrent consumers
        await asyncio.gather(
            consume(10),
            consume(10),
            consume(10)
        )

        # Total should be 30
        assert child.usage.tokens_used == 30
        assert parent.usage.tokens_used == 30
