"""
Tests for HIL-04: State persistence and resume after approval.

Phase 3 Plan 00: Test infrastructure for state persistence.
Phase 3 Plan 04: Tests for state serialization and restoration.

Tests cover:
- State serialization/deserialization
- Checkpoint creation and restoration
- Task status transitions during approval flow
"""

import pytest
from typing import Dict, Any


class TestStateSerialization:
    """Tests for state serialization/deserialization."""

    def test_serialize_state_basic(self):
        """State with messages can be serialized."""
        from src.security.approval_flow import serialize_state
        from langchain_core.messages import HumanMessage

        state = {
            "task_id": "task-1",
            "owner_user_id": "user-1",
            "messages": [HumanMessage(content="Hello")]
        }

        serialized = serialize_state(state)

        assert serialized["task_id"] == "task-1"
        assert serialized["owner_user_id"] == "user-1"
        assert len(serialized["messages"]) == 1
        assert serialized["messages"][0]["type"] == "HumanMessage"
        assert serialized["messages"][0]["content"] == "Hello"

    def test_serialize_state_preserves_messages(self):
        """Serialized messages include type, content, tool_calls."""
        from src.security.approval_flow import serialize_state
        from langchain_core.messages import HumanMessage, AIMessage

        state = {
            "task_id": "task-1",
            "messages": [
                HumanMessage(content="Hello"),
                AIMessage(content="Hi there", tool_calls=[{"id": "call-1", "name": "test", "args": {}}])
            ]
        }

        serialized = serialize_state(state)

        assert len(serialized["messages"]) == 2
        assert serialized["messages"][0]["type"] == "HumanMessage"
        assert serialized["messages"][1]["type"] == "AIMessage"
        assert len(serialized["messages"][1]["tool_calls"]) == 1

    def test_deserialize_state_basic(self):
        """Serialized state can be deserialized."""
        from src.security.approval_flow import deserialize_state
        from langchain_core.messages import HumanMessage

        serialized = {
            "task_id": "task-1",
            "owner_user_id": "user-1",
            "messages": [
                {"type": "HumanMessage", "content": "Hello"}
            ]
        }

        state = deserialize_state(serialized)

        assert state["task_id"] == "task-1"
        assert len(state["messages"]) == 1
        assert isinstance(state["messages"][0], HumanMessage)
        assert state["messages"][0].content == "Hello"

    def test_deserialize_state_preserves_message_types(self):
        """Message types are restored correctly (HumanMessage, AIMessage, ToolMessage)."""
        from src.security.approval_flow import deserialize_state
        from langchain_core.messages import HumanMessage, AIMessage, ToolMessage

        serialized = {
            "messages": [
                {"type": "HumanMessage", "content": "Hello"},
                {"type": "AIMessage", "content": "Hi", "tool_calls": [{"id": "1", "name": "test", "args": {}}]},
                {"type": "ToolMessage", "content": "result", "tool_call_id": "1", "name": "test"}
            ]
        }

        state = deserialize_state(serialized)

        assert isinstance(state["messages"][0], HumanMessage)
        assert isinstance(state["messages"][1], AIMessage)
        assert isinstance(state["messages"][2], ToolMessage)
        assert len(state["messages"][1].tool_calls) == 1

    def test_serialize_deserialize_roundtrip(self):
        """State can be serialized and deserialized with full fidelity."""
        from src.security.approval_flow import serialize_state, deserialize_state
        from langchain_core.messages import HumanMessage, AIMessage

        original = {
            "task_id": "task-123",
            "owner_user_id": "user-456",
            "intent": "Task",
            "plan": {"step": 1},
            "messages": [
                HumanMessage(content="Hello"),
                AIMessage(content="Hi there", tool_calls=[{"id": "call-1", "name": "test", "args": {"arg": "value"}}])
            ]
        }

        serialized = serialize_state(original)
        deserialized = deserialize_state(serialized)

        assert deserialized["task_id"] == "task-123"
        assert deserialized["owner_user_id"] == "user-456"
        assert deserialized["intent"] == "Task"
        assert deserialized["plan"] == {"step": 1}
        assert len(deserialized["messages"]) == 2
        assert isinstance(deserialized["messages"][0], HumanMessage)
        assert isinstance(deserialized["messages"][1], AIMessage)
        assert deserialized["messages"][1].tool_calls[0]["id"] == "call-1"


class TestApprovalResume:
    """Tests for HIL-04: State persistence and resume after approval."""

    def test_state_saved_to_checkpoints(self, mock_settings, temp_db):
        """Pending state must be saved to TaskRecord.checkpoints."""
        from src.security.approval_flow import (
            ApprovalRequest,
            save_pending_state
        )
        from src.core.db import TaskRecord, save_task, get_task
        from langchain_core.messages import HumanMessage

        task = TaskRecord(
            task_id="task-save-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="Running"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-save-test",
            owner_user_id="user-123"
        )

        state = {
            "task_id": "task-save-test",
            "owner_user_id": "user-123",
            "messages": [HumanMessage(content="test")]
        }

        snapshot_id = save_pending_state(state, request)

        saved_task = get_task("task-save-test")
        assert saved_task.status == "WaitingApproval"
        assert saved_task.pending_approval_id == request.approval_id
        assert len(saved_task.checkpoints) > 0
        assert snapshot_id.startswith("snapshot_")

    def test_state_restored_correctly(self, mock_settings, temp_db):
        """State must be restored correctly from TaskRecord."""
        from src.security.approval_flow import (
            ApprovalRequest,
            save_pending_state,
            restore_state
        )
        from src.core.db import TaskRecord, save_task
        from langchain_core.messages import HumanMessage

        task = TaskRecord(
            task_id="task-restore-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="Running"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-restore-test",
            owner_user_id="user-123"
        )

        state = {
            "task_id": "task-restore-test",
            "owner_user_id": "user-123",
            "messages": [HumanMessage(content="test message")]
        }

        save_pending_state(state, request)
        restored = restore_state("task-restore-test", request.approval_id)

        assert restored is not None
        assert restored["task_id"] == "task-restore-test"
        assert len(restored["messages"]) == 1
        assert restored["messages"][0].content == "test message"

    def test_state_includes_required_fields(self, mock_settings, temp_db):
        """State includes messages, pending_tool, approval_id."""
        from src.security.approval_flow import (
            ApprovalRequest,
            save_pending_state
        )
        from src.core.db import TaskRecord, save_task, get_task
        from langchain_core.messages import HumanMessage

        task = TaskRecord(
            task_id="task-fields-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="Running"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-fields-test",
            owner_user_id="user-123"
        )

        state = {
            "task_id": "task-fields-test",
            "owner_user_id": "user-123",
            "intent": "Task",
            "plan": {"step": 1},
            "messages": [HumanMessage(content="test")]
        }

        save_pending_state(state, request)

        saved_task = get_task("task-fields-test")
        checkpoint = saved_task.checkpoints[-1]

        assert checkpoint.get("tool_name") == "execute_command"
        assert checkpoint.get("approval_id") == request.approval_id
        assert "state_snapshot" in checkpoint

    def test_restored_state_preserves_tool_call_id(self, mock_settings, temp_db):
        """Restored state preserves tool_call_id for ToolMessage."""
        from src.security.approval_flow import (
            ApprovalRequest,
            save_pending_state,
            restore_state
        )
        from src.core.db import TaskRecord, save_task
        from langchain_core.messages import HumanMessage, AIMessage, ToolMessage

        task = TaskRecord(
            task_id="task-toolcall-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="Running"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-123",
            task_id="task-toolcall-test",
            owner_user_id="user-123"
        )

        state = {
            "task_id": "task-toolcall-test",
            "owner_user_id": "user-123",
            "messages": [
                HumanMessage(content="Hello"),
                AIMessage(content="", tool_calls=[{"id": "tc-1", "name": "test", "args": {}}]),
                ToolMessage(content="result", tool_call_id="tc-1", name="test")
            ]
        }

        save_pending_state(state, request)
        restored = restore_state("task-toolcall-test", request.approval_id)

        assert restored is not None
        tool_msg = restored["messages"][2]
        assert isinstance(tool_msg, ToolMessage)
        assert tool_msg.tool_call_id == "tc-1"

    def test_task_status_transitions(self, mock_settings, temp_db):
        """Task status changes to WaitingApproval during approval."""
        from src.security.approval_flow import (
            ApprovalRequest,
            save_pending_state,
            restore_state
        )
        from src.core.db import TaskRecord, save_task, get_task
        from langchain_core.messages import HumanMessage

        task = TaskRecord(
            task_id="task-status-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="Running"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-status-test",
            owner_user_id="user-123"
        )

        state = {
            "task_id": "task-status-test",
            "owner_user_id": "user-123",
            "messages": [HumanMessage(content="test")]
        }

        # Before: Running
        assert get_task("task-status-test").status == "Running"

        # Save state
        save_pending_state(state, request)

        # After save: WaitingApproval
        assert get_task("task-status-test").status == "WaitingApproval"

        # Restore state
        restore_state("task-status-test", request.approval_id)

        # After restore: Running
        assert get_task("task-status-test").status == "Running"
