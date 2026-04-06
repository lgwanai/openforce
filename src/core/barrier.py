"""
Barrier for concurrent agent result collection.

Implements ORC-01: Collect results from parallel agents
Implements ORC-02: Timeout release to prevent deadlocks
"""

import asyncio
from typing import Dict, Any, List, Optional, Callable
from dataclasses import dataclass, field
from datetime import datetime
import logging

logger = logging.getLogger(__name__)


@dataclass
class AgentResult:
    """Result from a single agent execution."""
    agent_id: str
    success: bool
    result: Any
    error: Optional[str] = None
    duration_ms: float = 0.0
    timestamp: str = field(default_factory=lambda: datetime.utcnow().isoformat())


class Barrier:
    """
    Concurrent barrier for collecting agent results.
    """

    def __init__(
        self,
        expected: int,
        timeout: float = 60.0,
        on_timeout: Optional[Callable[[List[str]], None]] = None
    ):
        self.expected = expected
        self.timeout = timeout
        self.on_timeout = on_timeout
        self._results: Dict[str, AgentResult] = {}
        self._event = asyncio.Event()
        self._started_at = datetime.utcnow()
        self._completed = False

    def submit(self, agent_id: str, result: AgentResult) -> bool:
        if self._completed:
            return False
        self._results[agent_id] = result
        if len(self._results) >= self.expected:
            self._completed = True
            self._event.set()
            return True
        return False

    async def wait(self) -> Dict[str, AgentResult]:
        try:
            await asyncio.wait_for(self._event.wait(), timeout=self.timeout)
        except asyncio.TimeoutError:
            self._completed = True
            missing = [f"agent_{i}" for i in range(self.expected)
                      if f"agent_{i}" not in self._results]
            if self.on_timeout:
                self.on_timeout(missing)
        return self._results.copy()

    @property
    def is_complete(self) -> bool:
        return self._completed

    @property
    def missing_count(self) -> int:
        return max(0, self.expected - len(self._results))


class BarrierManager:
    def __init__(self):
        self._barriers: Dict[str, Barrier] = {}

    def create(self, barrier_id: str, expected: int, timeout: float = 60.0,
               on_timeout: Optional[Callable] = None) -> Barrier:
        barrier = Barrier(expected, timeout, on_timeout)
        self._barriers[barrier_id] = barrier
        return barrier

    def get(self, barrier_id: str) -> Optional[Barrier]:
        return self._barriers.get(barrier_id)

    def remove(self, barrier_id: str) -> bool:
        if barrier_id in self._barriers:
            del self._barriers[barrier_id]
            return True
        return False

    def active_count(self) -> int:
        return sum(1 for b in self._barriers.values() if not b.is_complete)
