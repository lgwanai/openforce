"""
Taint propagation and sensitive tool verification protocol.

This module provides taint tracking for security-sensitive operations:
- trust_level: Trusted | Derived | Untrusted
- taint_source: web | search | upload | user_free_text | internal

For approval token generation and verification, see src.security.approval module.
"""

import os
import time
from typing import Dict, Any

from .approval import ApprovalTokenManager


class TaintEngine:
    """
    Taint propagation and sensitive tool verification protocol.

    trust_level: Trusted | Derived | Untrusted
    taint_source: web | search | upload | user_free_text | internal
    """

    @staticmethod
    def get_trust_level(sources: list[str]) -> str:
        if "web" in sources or "upload" in sources:
            return "Untrusted"
        if "user_free_text" in sources:
            return "Derived"
        return "Trusted"

    @staticmethod
    def check_tool_call(tool_name: str, args: Dict[str, Any], trust_level: str) -> bool:
        """
        Hard constraint on execution gateway.
        """
        high_risk_tools = ["execute_command", "delete_file", "write_api"]
        if tool_name in high_risk_tools:
            # Must require human approval, no exemptions.
            return False

        return True


# Default instance with env var secret
_default_manager = ApprovalTokenManager(
    secret_key=os.environ.get('OPENFORCE_APPROVAL_SECRET_KEY', '').encode() or None
)


def generate_approval_token(
    owner_user_id: str,
    task_id: str,
    approval_id: str,
    action_hash: str,
    exp: int,
    nonce: str,
    channel_binding_hash: str
) -> str:
    """
    Generate approval token using HMAC-SHA256 signature.

    This function delegates to ApprovalTokenManager for secure token generation.

    Args:
        owner_user_id: ID of the user who owns this approval
        task_id: ID of the task requiring approval
        approval_id: Unique approval request ID
        action_hash: Hash of the action being approved
        exp: Token expiration timestamp (Unix epoch)
        nonce: Random nonce for uniqueness
        channel_binding_hash: Channel binding for additional security

    Returns:
        Signed token string (format: <timestamp>:<nonce>:<hmac_signature>)
    """
    # Convert exp (absolute timestamp) to expires_in (relative duration)
    expires_in = exp - int(time.time())
    if expires_in < 0:
        expires_in = 0  # Already expired, but generate anyway for verification

    return _default_manager.generate_token(
        owner_user_id=owner_user_id,
        task_id=task_id,
        approval_id=approval_id,
        action_hash=action_hash,
        expires_in=expires_in,
        nonce=nonce,
        channel_binding_hash=channel_binding_hash
    )


def verify_approval_token(
    token: str,
    owner_user_id: str,
    task_id: str,
    approval_id: str,
    action_hash: str,
    exp: int,
    nonce: str,
    channel_binding_hash: str
) -> bool:
    """
    Verify approval token using constant-time comparison.

    This function delegates to ApprovalTokenManager for secure token verification.
    Uses hmac.compare_digest() internally to prevent timing attacks.

    Args:
        token: Token string to verify
        owner_user_id: Expected owner user ID
        task_id: Expected task ID
        approval_id: Expected approval ID
        action_hash: Expected action hash
        exp: Expected expiration timestamp (Unix epoch)
        nonce: Expected nonce
        channel_binding_hash: Expected channel binding hash

    Returns:
        True if token is valid and not expired, False otherwise
    """
    return _default_manager.verify_token(
        token=token,
        owner_user_id=owner_user_id,
        task_id=task_id,
        approval_id=approval_id,
        action_hash=action_hash,
        channel_binding_hash=channel_binding_hash
    )
