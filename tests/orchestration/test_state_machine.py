"""Tests for ORC-03: State machine."""
import pytest

from src.core.state_machine import TaskStateMachine, TaskState, TRANSITIONS


class TestTaskState:
    """Tests for TaskState enum."""

    def test_all_states_defined(self):
        """All required states should be defined."""
        required = ["Pending", "Running", "WaitingApproval", "WaitingInput",
                   "Paused", "Completed", "Failed", "Cancelled"]
        for state in required:
            assert TaskState(state)


class TestTaskStateMachine:
    """Tests for TaskStateMachine."""

    def test_initial_state(self):
        """Machine should start with PENDING state."""
        sm = TaskStateMachine()
        assert sm.state == TaskState.PENDING

    def test_valid_transition(self):
        """Valid transition should succeed."""
        sm = TaskStateMachine()
        assert sm.transition(TaskState.RUNNING, "Starting") == True
        assert sm.state == TaskState.RUNNING

    def test_invalid_transition(self):
        """Invalid transition should fail."""
        sm = TaskStateMachine()
        assert sm.transition(TaskState.COMPLETED, "Skip") == False
        assert sm.state == TaskState.PENDING

    def test_terminal_state_detection(self):
        """Terminal states should be detected."""
        sm = TaskStateMachine(TaskState.COMPLETED)
        assert sm.is_terminal == True

        sm = TaskStateMachine(TaskState.RUNNING)
        assert sm.is_terminal == False

    def test_history_tracking(self):
        """History should record all transitions."""
        sm = TaskStateMachine()
        sm.transition(TaskState.RUNNING, "Start")
        sm.transition(TaskState.WAITING_APPROVAL, "Need approval")

        assert len(sm.history) == 2
        assert sm.history[0].from_state == TaskState.PENDING
        assert sm.history[0].to_state == TaskState.RUNNING

    def test_force_transition(self):
        """Force transition should work regardless of validity."""
        sm = TaskStateMachine()
        sm.force_transition(TaskState.COMPLETED, "Recovery")
        assert sm.state == TaskState.COMPLETED

    def test_valid_transitions_list(self):
        """Should list valid next states."""
        sm = TaskStateMachine()
        valid = sm.get_valid_transitions()
        assert TaskState.RUNNING in valid
        assert TaskState.CANCELLED in valid
        assert TaskState.COMPLETED not in valid

    def test_full_lifecycle(self):
        """Test a full task lifecycle."""
        sm = TaskStateMachine()

        sm.transition(TaskState.RUNNING, "Start")
        assert sm.state == TaskState.RUNNING

        sm.transition(TaskState.WAITING_APPROVAL, "Need approval")
        assert sm.state == TaskState.WAITING_APPROVAL

        sm.transition(TaskState.RUNNING, "Approved")
        assert sm.state == TaskState.RUNNING

        sm.transition(TaskState.COMPLETED, "Done")
        assert sm.state == TaskState.COMPLETED
        assert sm.is_terminal

    def test_from_status_string(self):
        """Should create from status string."""
        sm = TaskStateMachine.from_status_string("Running")
        assert sm.state == TaskState.RUNNING

    def test_metadata(self):
        """Should store and retrieve metadata."""
        sm = TaskStateMachine()
        sm.set_metadata("retry_count", 3)
        assert sm.get_metadata("retry_count") == 3
        assert sm.get_metadata("missing", "default") == "default"
