"""
Approval workflow for high-risk tool execution.

This module provides:
- ApprovalRequest: Exception raised when approval is needed
- compute_action_hash: Canonical hash for TOCTOU protection
- verify_action_hash: Verify action matches approved hash
- generate_approval_for_request: Generate approval token and update task
- consume_approval_token: Atomic token consumption with replay protection

Security properties:
- Action hashes use canonical JSON to prevent hash collisions
- HMAC-SHA256 tokens for approval authentication
- Timing-attack resistant hash comparison
- Atomic nonce consumption prevents replay attacks
"""

import hashlib
import json
import secrets
import hmac
from dataclasses import dataclass
from typing import Dict, Any, Optional

from src.security.approval import ApprovalTokenManager
from src.core.db import get_task, save_task, consume_nonce


@dataclass
class ApprovalRequest(Exception):
    """Raised when an operation needs user approval."""
    tool_name: str
    tool_args: Dict[str, Any]
    tool_call_id: str
    action_hash: str
    approval_id: str
    task_id: str
    owner_user_id: str
    snapshot: Optional[Dict[str, Any]] = None

    @classmethod
    def from_tool_call(
        cls,
        tool_name: str,
        tool_args: Dict[str, Any],
        tool_call_id: str,
        task_id: str,
        owner_user_id: str,
        state_snapshot: Optional[Dict[str, Any]] = None
    ) -> 'ApprovalRequest':
        """
        Create an ApprovalRequest from a tool call.

        Args:
            tool_name: Name of the tool being called
            tool_args: Tool arguments
            tool_call_id: Unique ID for this tool call
            task_id: Task ID for context
            owner_user_id: Owner user ID for authorization
            state_snapshot: Optional agent state snapshot for resume

        Returns:
            ApprovalRequest instance
        """
        # Canonical JSON for TOCTOU protection
        action_hash = compute_action_hash(tool_name, tool_args, task_id)
        approval_id = f"apr_{secrets.token_urlsafe(8)}"

        return cls(
            tool_name=tool_name,
            tool_args=tool_args,
            tool_call_id=tool_call_id,
            action_hash=action_hash,
            approval_id=approval_id,
            task_id=task_id,
            owner_user_id=owner_user_id,
            snapshot=state_snapshot
        )

    def to_dict(self) -> Dict[str, Any]:
        """Serialize ApprovalRequest for API/storage."""
        return {
            "tool_name": self.tool_name,
            "tool_args": self.tool_args,
            "tool_call_id": self.tool_call_id,
            "action_hash": self.action_hash,
            "approval_id": self.approval_id,
            "task_id": self.task_id,
            "owner_user_id": self.owner_user_id
        }


def compute_action_hash(
    tool_name: str,
    tool_args: Dict[str, Any],
    task_id: str
) -> str:
    """
    Compute canonical SHA256 hash of tool call for TOCTOU protection.

    Uses canonical JSON serialization:
    - Keys sorted alphabetically
    - No whitespace (separators=(',', ':'))
    - UTF-8 encoding

    Args:
        tool_name: Name of the tool being called
        tool_args: Tool arguments dictionary
        task_id: Task ID for context binding

    Returns:
        SHA256 hex digest (64 characters)
    """
    # Canonical JSON serialization
    canonical = json.dumps({
        "tool": tool_name,
        "args": tool_args,
        "task_id": task_id
    }, sort_keys=True, separators=(',', ':'))

    return hashlib.sha256(canonical.encode()).hexdigest()


def verify_action_hash(
    expected_hash: str,
    tool_name: str,
    tool_args: Dict[str, Any],
    task_id: str
) -> bool:
    """
    Verify that the action hash matches the tool call.

    This is used to detect TOCTOU attacks where the tool call
    is modified between approval and execution.

    Args:
        expected_hash: The hash from the approval request
        tool_name: Name of the tool being executed
        tool_args: Tool arguments being executed
        task_id: Task ID for context binding

    Returns:
        True if hash matches, False otherwise
    """
    computed_hash = compute_action_hash(tool_name, tool_args, task_id)
    return hmac.compare_digest(expected_hash, computed_hash)


def generate_approval_for_request(request: ApprovalRequest) -> Dict[str, Any]:
    """
    Generate approval token and data for user confirmation.

    Args:
        request: The ApprovalRequest to generate approval for

    Returns:
        Dict with approval_id, token, tool_name, tool_args, tool_call_id, action_hash, expires_in
    """
    manager = ApprovalTokenManager()

    token = manager.generate_token(
        owner_user_id=request.owner_user_id,
        task_id=request.task_id,
        approval_id=request.approval_id,
        action_hash=request.action_hash,
        expires_in=3600  # 1 hour
    )

    # Save pending state to task
    task = get_task(request.task_id)
    if task:
        task.pending_approval_id = request.approval_id
        task.approval_action_hash = request.action_hash
        task.status = "WaitingApproval"
        task.checkpoints.append({
            "type": "approval_pending",
            "approval_id": request.approval_id,
            "tool_name": request.tool_name,
            "tool_args": request.tool_args,
            "tool_call_id": request.tool_call_id,
            "action_hash": request.action_hash,
            "snapshot": request.snapshot
        })
        save_task(task)

    return {
        "approval_id": request.approval_id,
        "token": token,
        "tool_name": request.tool_name,
        "tool_args": request.tool_args,
        "tool_call_id": request.tool_call_id,
        "action_hash": request.action_hash,
        "expires_in": 3600
    }


def consume_approval_token(
    token: str,
    approval_id: str,
    task_id: str,
    owner_user_id: str,
    action_hash: str,
    channel_binding_hash: Optional[str] = None
) -> Dict[str, Any]:
    """
    Verify approval token and consume it atomically.

    This function combines token verification and nonce consumption
    into a single atomic operation to prevent replay attacks.

    Args:
        token: Token string to verify and consume
        approval_id: Expected approval ID
        task_id: Expected task ID
        owner_user_id: Expected owner user ID
        action_hash: Expected action hash
        channel_binding_hash: Optional channel binding hash

    Returns:
        Approval checkpoint data if token is valid

    Raises:
        ValueError: If token is invalid, expired, or already consumed
    """
    manager = ApprovalTokenManager()

    # Extract nonce from token (format: <exp>:<nonce>:<signature>)
    parts = token.split(':')
    if len(parts) != 3:
        raise ValueError("Invalid token format")

    exp_str, nonce, signature = parts

    # Verify token signature FIRST (before any database operations)
    if not manager.verify_token(
        token=token,
        owner_user_id=owner_user_id,
        task_id=task_id,
        approval_id=approval_id,
        action_hash=action_hash,
        channel_binding_hash=channel_binding_hash
    ):
        raise ValueError("Invalid token signature or expired token")

    # Atomic nonce consumption (prevents replay)
    if not consume_nonce(nonce):
        raise ValueError("Token already used (replay attack detected)")

    # Load and return pending approval data
    task = get_task(task_id)
    if not task:
        raise ValueError(f"Task {task_id} not found")

    for checkpoint in reversed(task.checkpoints):
        if (checkpoint.get("type") == "approval_pending" and
            checkpoint.get("approval_id") == approval_id):
            return checkpoint

    raise ValueError(f"Approval {approval_id} not found in task {task_id}")
