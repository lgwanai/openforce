"""
Task state machine implementation.

Implements ORC-03: Support all design states
"""

from enum import Enum
from typing import Optional, List, Set
from dataclasses import dataclass
from datetime import datetime


class TaskState(Enum):
    """All supported task states."""
    PENDING = "Pending"
    RUNNING = "Running"
    WAITING_APPROVAL = "WaitingApproval"
    WAITING_INPUT = "WaitingInput"
    PAUSED = "Paused"
    COMPLETED = "Completed"
    FAILED = "Failed"
    CANCELLED = "Cancelled"


# Valid state transitions
TRANSITIONS: dict = {
    TaskState.PENDING: {TaskState.RUNNING, TaskState.CANCELLED},
    TaskState.RUNNING: {
        TaskState.COMPLETED,
        TaskState.FAILED,
        TaskState.WAITING_APPROVAL,
        TaskState.WAITING_INPUT,
        TaskState.PAUSED,
        TaskState.CANCELLED
    },
    TaskState.WAITING_APPROVAL: {TaskState.RUNNING, TaskState.FAILED, TaskState.CANCELLED},
    TaskState.WAITING_INPUT: {TaskState.RUNNING, TaskState.PAUSED, TaskState.CANCELLED},
    TaskState.PAUSED: {TaskState.RUNNING, TaskState.CANCELLED},
    TaskState.COMPLETED: set(),
    TaskState.FAILED: {TaskState.PENDING},
    TaskState.CANCELLED: set(),
}


@dataclass
class StateTransition:
    """Record of a state transition."""
    from_state: TaskState
    to_state: TaskState
    reason: str
    timestamp: str = ""

    def __post_init__(self):
        if not self.timestamp:
            self.timestamp = datetime.utcnow().isoformat()


class TaskStateMachine:
    """State machine for task lifecycle management."""

    def __init__(self, initial_state: TaskState = TaskState.PENDING):
        self._state = initial_state
        self._history: List[StateTransition] = []
        self._metadata: dict = {}

    @property
    def state(self) -> TaskState:
        return self._state

    @property
    def is_terminal(self) -> bool:
        return len(TRANSITIONS.get(self._state, set())) == 0

    @property
    def history(self) -> List[StateTransition]:
        return self._history.copy()

    def can_transition_to(self, new_state: TaskState) -> bool:
        return new_state in TRANSITIONS.get(self._state, set())

    def transition(self, new_state: TaskState, reason: str = "") -> bool:
        if not self.can_transition_to(new_state):
            return False
        old_state = self._state
        self._history.append(StateTransition(from_state=old_state, to_state=new_state, reason=reason))
        self._state = new_state
        return True

    def force_transition(self, new_state: TaskState, reason: str = ""):
        old_state = self._state
        self._history.append(StateTransition(
            from_state=old_state, to_state=new_state, reason=f"FORCED: {reason}"
        ))
        self._state = new_state

    def set_metadata(self, key: str, value):
        self._metadata[key] = value

    def get_metadata(self, key: str, default=None):
        return self._metadata.get(key, default)

    def get_valid_transitions(self) -> List[TaskState]:
        return list(TRANSITIONS.get(self._state, set()))

    @classmethod
    def from_status_string(cls, status: str) -> 'TaskStateMachine':
        state = TaskState(status)
        return cls(state)
