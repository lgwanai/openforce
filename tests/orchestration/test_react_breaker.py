"""Tests for ORC-04: ReAct loop breaker."""
import pytest

from src.core.react_breaker import ReactBreaker, ReActStep, BreakerResult


class TestReActStep:
    """Tests for ReActStep."""

    def test_fingerprint(self):
        """Fingerprint should be consistent for same inputs."""
        s1 = ReActStep(action="tool", action_input={"x": 1})
        s2 = ReActStep(action="tool", action_input={"x": 1})
        assert s1.fingerprint() == s2.fingerprint()

    def test_fingerprint_different(self):
        """Different actions should have different fingerprints."""
        s1 = ReActStep(action="tool1")
        s2 = ReActStep(action="tool2")
        assert s1.fingerprint() != s2.fingerprint()


class TestReactBreaker:
    """Tests for ReactBreaker."""

    def test_no_loop_detected(self):
        """Should not break when no loop."""
        breaker = ReactBreaker(max_same_action=3)

        for i in range(3):
            breaker.add_step(action=f"tool_{i}")
            result = breaker.check()
            assert not result.should_break

    def test_detects_repeated_action(self):
        """Should detect repeated same action."""
        breaker = ReactBreaker(max_same_action=3)

        breaker.add_step(action="same_tool")
        breaker.add_step(action="same_tool")
        breaker.add_step(action="same_tool")

        result = breaker.check()
        assert result.should_break
        assert "same_tool" in result.reason

    def test_detects_max_steps(self):
        """Should break on max steps."""
        breaker = ReactBreaker(max_total_steps=5)

        for i in range(6):
            breaker.add_step(action=f"tool_{i}")

        result = breaker.check()
        assert result.should_break
        assert "Max steps" in result.reason

    def test_detects_2_step_cycle(self):
        """Should detect 2-step cycle pattern."""
        # Use max_same_action=4 so cycle detection triggers before consecutive action check
        breaker = ReactBreaker(max_same_action=4)

        # Create a 2-step cycle: A, B, A, B
        breaker.add_step(action="tool_a", action_input={"x": 1})
        breaker.add_step(action="tool_b", action_input={"y": 2})
        breaker.add_step(action="tool_a", action_input={"x": 1})
        breaker.add_step(action="tool_b", action_input={"y": 2})
        breaker.add_step(action="tool_a", action_input={"x": 1})
        breaker.add_step(action="tool_b", action_input={"y": 2})

        result = breaker.check()
        assert result.should_break
        assert "cycle" in result.reason.lower()

    def test_reset(self):
        """Reset should clear history."""
        breaker = ReactBreaker()
        breaker.add_step(action="tool")
        assert breaker.step_count == 1

        breaker.reset()
        assert breaker.step_count == 0

    def test_get_recent_steps(self):
        """Should return recent steps."""
        breaker = ReactBreaker()
        for i in range(10):
            breaker.add_step(action=f"tool_{i}")

        recent = breaker.get_recent_steps(3)
        assert len(recent) == 3
        assert recent[0].action == "tool_7"

    def test_no_break_on_different_actions(self):
        """Should not break when actions are different."""
        breaker = ReactBreaker(max_same_action=3)

        for i in range(10):
            breaker.add_step(action=f"unique_tool_{i}")
            result = breaker.check()
            assert not result.should_break
