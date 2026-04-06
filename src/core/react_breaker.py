"""
ReAct loop breaker implementation.

Implements ORC-04: Detect and break ReAct dead loops
"""

from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field
from datetime import datetime
import hashlib
import logging

logger = logging.getLogger(__name__)


@dataclass
class ReActStep:
    """Single step in a ReAct loop."""
    thought: Optional[str] = None
    action: Optional[str] = None
    action_input: Optional[Dict[str, Any]] = None
    observation: Optional[str] = None
    timestamp: str = field(default_factory=lambda: datetime.utcnow().isoformat())

    def fingerprint(self) -> str:
        """Generate fingerprint for deduplication."""
        parts = []
        if self.thought:
            parts.append(f"thought:{self.thought[:100]}")
        if self.action:
            parts.append(f"action:{self.action}")
        if self.action_input:
            input_str = str(sorted(self.action_input.items()))
            parts.append(f"input:{hashlib.md5(input_str.encode()).hexdigest()[:8]}")
        return "|".join(parts)


@dataclass
class BreakerResult:
    """Result of loop detection."""
    should_break: bool
    reason: str
    loop_count: int = 0
    repeated_pattern: Optional[str] = None


class ReactBreaker:
    """Detects and breaks ReAct dead loops."""

    def __init__(
        self,
        max_same_action: int = 3,
        max_same_pattern: int = 2,
        max_total_steps: int = 50,
        window_size: int = 10
    ):
        self.max_same_action = max_same_action
        self.max_same_pattern = max_same_pattern
        self.max_total_steps = max_total_steps
        self.window_size = window_size
        self._steps: List[ReActStep] = []

    def add_step(
        self,
        thought: Optional[str] = None,
        action: Optional[str] = None,
        action_input: Optional[Dict[str, Any]] = None,
        observation: Optional[str] = None
    ) -> None:
        step = ReActStep(
            thought=thought, action=action, action_input=action_input, observation=observation
        )
        self._steps.append(step)

    def check(self) -> BreakerResult:
        if not self._steps:
            return BreakerResult(should_break=False, reason="No steps recorded")

        # Check total steps
        if len(self._steps) >= self.max_total_steps:
            return BreakerResult(
                should_break=True, reason=f"Max steps ({self.max_total_steps}) reached",
                loop_count=len(self._steps)
            )

        # Check consecutive same action
        recent = self._steps[-self.window_size:]
        actions = [s.action for s in recent if s.action]

        if actions:
            last_action = actions[-1]
            consecutive = sum(1 for a in reversed(actions) if a == last_action)
            if consecutive >= self.max_same_action:
                return BreakerResult(
                    should_break=True, reason=f"Action '{last_action}' repeated {consecutive} times",
                    loop_count=consecutive, repeated_pattern=last_action
                )

        # Check cycle patterns
        if len(self._steps) >= 6:
            result = self._detect_cycle_pattern()
            if result.should_break:
                return result

        return BreakerResult(should_break=False, reason="No loop detected")

    def _detect_cycle_pattern(self) -> BreakerResult:
        fingerprints = [s.fingerprint() for s in self._steps[-10:]]

        # 2-step cycle
        if len(fingerprints) >= 4:
            if (fingerprints[-1] == fingerprints[-3] and
                fingerprints[-2] == fingerprints[-4] and
                fingerprints[-1] != fingerprints[-2]):
                return BreakerResult(
                    should_break=True, reason="Detected 2-step cycle", loop_count=2,
                    repeated_pattern=f"{fingerprints[-2]} -> {fingerprints[-1]}"
                )

        # 3-step cycle
        if len(fingerprints) >= 6:
            if (fingerprints[-1] == fingerprints[-4] and
                fingerprints[-2] == fingerprints[-5] and
                fingerprints[-3] == fingerprints[-6]):
                return BreakerResult(
                    should_break=True, reason="Detected 3-step cycle", loop_count=3
                )

        return BreakerResult(should_break=False, reason="No cycle pattern")

    def reset(self) -> None:
        self._steps = []

    @property
    def step_count(self) -> int:
        return len(self._steps)

    def get_recent_steps(self, n: int = 5) -> List[ReActStep]:
        return self._steps[-n:]
