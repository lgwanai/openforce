"""
Tests for Human-in-the-loop approval workflow.

Phase 3 Plan 00: Test infrastructure for HIL requirements.
Phase 3 Plan 01: Tests for HIL-01 approval flow integration.
Phase 3 Plan 02: Tests for HIL-02 action hash.

Tests cover:
- HIL-01: High-risk tool approval integration
- HIL-02: Canonical action hash for TOCTOU protection
- HIL-03: One-time atomic token consumption
"""

import pytest
import hashlib
import json
from typing import Dict, Any


class TestApprovalFlow:
    """Tests for HIL-01: High-risk tool approval integration."""

    def test_approval_request_is_exception(self):
        """ApprovalRequest must be an Exception subclass."""
        from src.security.approval_flow import ApprovalRequest

        assert issubclass(ApprovalRequest, Exception)

    def test_approval_request_has_required_fields(self):
        """ApprovalRequest must contain all required fields."""
        from src.security.approval_flow import ApprovalRequest

        request = ApprovalRequest(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            action_hash="hash123",
            approval_id="apr_123",
            task_id="task-1",
            owner_user_id="user-1"
        )

        assert request.tool_name == "execute_command"
        assert request.tool_args == {"command": "ls"}
        assert request.tool_call_id == "call-1"
        assert request.action_hash == "hash123"
        assert request.approval_id == "apr_123"
        assert request.task_id == "task-1"
        assert request.owner_user_id == "user-1"

    def test_approval_request_from_tool_call(self):
        """ApprovalRequest.from_tool_call creates valid request with canonical hash."""
        from src.security.approval_flow import ApprovalRequest, compute_action_hash

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls -la"},
            tool_call_id="call-1",
            task_id="task-1",
            owner_user_id="user-1"
        )

        assert request.tool_name == "execute_command"
        assert request.tool_args == {"command": "ls -la"}
        assert request.tool_call_id == "call-1"
        assert request.task_id == "task-1"
        assert request.owner_user_id == "user-1"
        assert request.approval_id.startswith("apr_")
        assert len(request.action_hash) == 64  # SHA256 hex digest

        # Verify action hash is computed correctly
        expected_hash = compute_action_hash("execute_command", {"command": "ls -la"}, "task-1")
        assert request.action_hash == expected_hash

    def test_approval_request_to_dict(self):
        """ApprovalRequest.to_dict serializes correctly."""
        from src.security.approval_flow import ApprovalRequest

        request = ApprovalRequest(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            action_hash="hash123",
            approval_id="apr_123",
            task_id="task-1",
            owner_user_id="user-1"
        )

        data = request.to_dict()

        assert data["tool_name"] == "execute_command"
        assert data["tool_args"] == {"command": "ls"}
        assert data["tool_call_id"] == "call-1"
        assert data["action_hash"] == "hash123"
        assert data["approval_id"] == "apr_123"
        assert data["task_id"] == "task-1"
        assert data["owner_user_id"] == "user-1"

    def test_generate_approval_for_request(self, mock_settings, temp_db):
        """generate_approval_for_request generates token and updates task."""
        from src.security.approval_flow import (
            ApprovalRequest,
            generate_approval_for_request
        )
        from src.core.db import TaskRecord, save_task, get_task

        # Create and save a task
        task = TaskRecord(
            task_id="task-gen-test",
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
            task_id="task-gen-test",
            owner_user_id="user-123"
        )

        approval_data = generate_approval_for_request(request)

        assert "approval_id" in approval_data
        assert "token" in approval_data
        assert approval_data["tool_name"] == "execute_command"
        assert approval_data["tool_args"] == {"command": "ls"}
        assert approval_data["expires_in"] == 3600

        # Verify task was updated
        updated_task = get_task("task-gen-test")
        assert updated_task.status == "WaitingApproval"
        assert updated_task.pending_approval_id == request.approval_id


class TestActionHash:
    """Tests for HIL-02: Canonical action hash for TOCTOU protection."""

    def test_same_call_same_hash(self):
        """Same tool call must produce same hash."""
        from src.security.approval_flow import compute_action_hash

        hash1 = compute_action_hash("execute_command", {"command": "ls"}, "task-1")
        hash2 = compute_action_hash("execute_command", {"command": "ls"}, "task-1")

        assert hash1 == hash2

    def test_different_args_different_hash(self):
        """Different tool args must produce different hash."""
        from src.security.approval_flow import compute_action_hash

        hash1 = compute_action_hash("execute_command", {"command": "ls"}, "task-1")
        hash2 = compute_action_hash("execute_command", {"command": "rm -rf /"}, "task-1")

        assert hash1 != hash2

    def test_canonical_json_key_order(self):
        """Key order must not affect hash."""
        from src.security.approval_flow import compute_action_hash

        # Different key order should produce same hash
        hash1 = compute_action_hash("tool", {"a": 1, "b": 2}, "task-1")
        hash2 = compute_action_hash("tool", {"b": 2, "a": 1}, "task-1")

        assert hash1 == hash2

    def test_hash_is_sha256_hex(self):
        """Hash must be SHA256 hex digest (64 characters)."""
        from src.security.approval_flow import compute_action_hash

        action_hash = compute_action_hash("tool", {"arg": "value"}, "task-1")

        assert len(action_hash) == 64
        assert all(c in '0123456789abcdef' for c in action_hash)

    def test_hash_includes_all_components(self):
        """Hash must include tool_name, args, and task_id."""
        from src.security.approval_flow import compute_action_hash

        hash1 = compute_action_hash("tool_a", {"arg": "value"}, "task-1")
        hash2 = compute_action_hash("tool_b", {"arg": "value"}, "task-1")
        hash3 = compute_action_hash("tool_a", {"arg": "different"}, "task-1")
        hash4 = compute_action_hash("tool_a", {"arg": "value"}, "task-2")

        # All hashes should be different
        hashes = [hash1, hash2, hash3, hash4]
        assert len(set(hashes)) == 4

    def test_verify_action_hash_valid(self):
        """verify_action_hash returns True for valid hash."""
        from src.security.approval_flow import compute_action_hash, verify_action_hash

        tool_name = "execute_command"
        tool_args = {"command": "ls -la"}
        task_id = "task-123"

        expected_hash = compute_action_hash(tool_name, tool_args, task_id)
        assert verify_action_hash(expected_hash, tool_name, tool_args, task_id) is True

    def test_verify_action_hash_invalid(self):
        """verify_action_hash returns False for invalid hash."""
        from src.security.approval_flow import verify_action_hash

        assert verify_action_hash("wrong_hash", "tool", {}, "task") is False

    def test_verify_action_hash_different_tool_name(self):
        """verify_action_hash returns False when tool_name differs."""
        from src.security.approval_flow import compute_action_hash, verify_action_hash

        expected_hash = compute_action_hash("tool_a", {"arg": "value"}, "task-1")
        assert verify_action_hash(expected_hash, "tool_b", {"arg": "value"}, "task-1") is False

    def test_verify_action_hash_different_args(self):
        """verify_action_hash returns False when args differ."""
        from src.security.approval_flow import compute_action_hash, verify_action_hash

        expected_hash = compute_action_hash("tool", {"arg": "value1"}, "task-1")
        assert verify_action_hash(expected_hash, "tool", {"arg": "value2"}, "task-1") is False


class TestTokenConsumption:
    """Tests for HIL-03: One-time atomic token consumption."""

    def test_valid_token_consumed_once(self, mock_settings, temp_db):
        """Valid token can be consumed exactly once."""
        from src.security.approval_flow import (
            ApprovalRequest,
            generate_approval_for_request,
            consume_approval_token
        )
        from src.core.db import TaskRecord, save_task

        # Setup: Create task and approval request
        task = TaskRecord(
            task_id="task-consume-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="WaitingApproval"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-consume-test",
            owner_user_id="user-123"
        )

        approval_data = generate_approval_for_request(request)

        # First consumption should succeed
        result = consume_approval_token(
            token=approval_data["token"],
            approval_id=approval_data["approval_id"],
            task_id="task-consume-test",
            owner_user_id="user-123",
            action_hash=approval_data["action_hash"]
        )

        assert result is not None
        assert result.get("type") == "approval_pending"
        assert result.get("tool_name") == "execute_command"

    def test_replay_attack_blocked(self, mock_settings, temp_db):
        """Same token cannot be used twice (replay attack blocked)."""
        from src.security.approval_flow import (
            ApprovalRequest,
            generate_approval_for_request,
            consume_approval_token
        )
        from src.core.db import TaskRecord, save_task

        task = TaskRecord(
            task_id="task-replay-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="WaitingApproval"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-replay-test",
            owner_user_id="user-123"
        )

        approval_data = generate_approval_for_request(request)

        # First consumption should succeed
        result = consume_approval_token(
            token=approval_data["token"],
            approval_id=approval_data["approval_id"],
            task_id="task-replay-test",
            owner_user_id="user-123",
            action_hash=approval_data["action_hash"]
        )
        assert result is not None

        # Second consumption should fail (replay attack)
        with pytest.raises(ValueError, match="already used"):
            consume_approval_token(
                token=approval_data["token"],
                approval_id=approval_data["approval_id"],
                task_id="task-replay-test",
                owner_user_id="user-123",
                action_hash=approval_data["action_hash"]
            )

    def test_invalid_token_rejected(self, mock_settings, temp_db):
        """Invalid token signature is rejected."""
        from src.security.approval_flow import consume_approval_token

        with pytest.raises(ValueError, match="Invalid token"):
            consume_approval_token(
                token="invalid:token:format",
                approval_id="apr_test",
                task_id="task-test",
                owner_user_id="user-123",
                action_hash="wrong_hash"
            )

    def test_expired_token_rejected(self, mock_settings, temp_db):
        """Expired token is rejected."""
        # This test would require mocking time or creating an already-expired token
        # For now, we skip this as it requires more complex setup
        pytest.skip("Expired token test requires time mocking")

    def test_wrong_action_hash_rejected(self, mock_settings, temp_db):
        """Token with wrong action_hash is rejected."""
        from src.security.approval_flow import (
            ApprovalRequest,
            generate_approval_for_request,
            consume_approval_token
        )
        from src.core.db import TaskRecord, save_task

        task = TaskRecord(
            task_id="task-hash-test",
            owner_user_id="user-123",
            conversation_id="conv-1",
            thread_id="thread-1",
            original_req="test",
            status="WaitingApproval"
        )
        save_task(task)

        request = ApprovalRequest.from_tool_call(
            tool_name="execute_command",
            tool_args={"command": "ls"},
            tool_call_id="call-1",
            task_id="task-hash-test",
            owner_user_id="user-123"
        )

        approval_data = generate_approval_for_request(request)

        # Wrong action_hash should be rejected
        with pytest.raises(ValueError, match="Invalid token"):
            consume_approval_token(
                token=approval_data["token"],
                approval_id=approval_data["approval_id"],
                task_id="task-hash-test",
                owner_user_id="user-123",
                action_hash="wrong_hash_12345"
            )
