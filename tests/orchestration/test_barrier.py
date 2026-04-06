"""Tests for ORC-01, ORC-02: Barrier concurrent collection."""
import pytest
import asyncio

from src.core.barrier import Barrier, AgentResult, BarrierManager


class TestBarrier:
    """Tests for Barrier class."""

    def test_barrier_collects_results(self):
        """Barrier should collect results from multiple agents."""
        async def run_test():
            barrier = Barrier(expected=3, timeout=5.0)

            barrier.submit("agent_0", AgentResult(agent_id="agent_0", success=True, result="r0"))
            barrier.submit("agent_1", AgentResult(agent_id="agent_1", success=True, result="r1"))
            assert not barrier.is_complete

            barrier.submit("agent_2", AgentResult(agent_id="agent_2", success=True, result="r2"))
            assert barrier.is_complete

            results = await barrier.wait()
            assert len(results) == 3

        asyncio.run(run_test())

    def test_barrier_timeout_release(self):
        """Barrier should release on timeout."""
        async def run_test():
            timeout_called = []

            def on_timeout(missing):
                timeout_called.append(missing)

            barrier = Barrier(expected=3, timeout=0.1, on_timeout=on_timeout)
            barrier.submit("agent_0", AgentResult(agent_id="agent_0", success=True, result="r0"))

            results = await barrier.wait()

            assert barrier.is_complete
            assert len(results) == 1
            assert len(timeout_called) == 1
            assert len(timeout_called[0]) == 2  # 2 missing

        asyncio.run(run_test())

    def test_barrier_missing_count(self):
        """Barrier should track missing count."""
        barrier = Barrier(expected=5)
        assert barrier.missing_count == 5

        barrier.submit("a", AgentResult(agent_id="a", success=True, result="r"))
        assert barrier.missing_count == 4

    def test_barrier_rejects_after_complete(self):
        """Barrier should reject submissions after complete."""
        barrier = Barrier(expected=1, timeout=5.0)
        barrier.submit("a", AgentResult(agent_id="a", success=True, result="r"))

        # Should reject new submission
        result = barrier.submit("b", AgentResult(agent_id="b", success=True, result="r2"))
        assert result == False


class TestBarrierManager:
    """Tests for BarrierManager."""

    def test_create_and_get(self):
        manager = BarrierManager()
        barrier = manager.create("test_barrier", expected=2)

        assert manager.get("test_barrier") == barrier
        assert manager.active_count() == 1

    def test_remove(self):
        manager = BarrierManager()
        manager.create("test", expected=2)

        assert manager.remove("test") == True
        assert manager.get("test") is None
        assert manager.remove("nonexistent") == False
