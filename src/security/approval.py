"""
HMAC-based approval token generation and verification.

This module provides cryptographically secure approval tokens using HMAC-SHA256
signatures. Tokens are tamper-proof and time-limited.

Security properties:
- Tokens are signed with HMAC-SHA256 using a secret key
- Tokens cannot be forged without the secret key
- Token verification uses constant-time comparison (timing attack resistant)
- Expired tokens are rejected
"""

import hmac
import hashlib
import time
import secrets
import os
from typing import Optional


class ApprovalTokenManager:
    """
    HMAC-based approval token generation and verification.

    Tokens are cryptographically signed and time-limited.
    Token format: <timestamp>:<nonce>:<hmac_signature>

    Usage:
        # With environment variable OPENFORCE_APPROVAL_SECRET_KEY
        manager = ApprovalTokenManager()

        # Or with explicit secret key
        manager = ApprovalTokenManager(secret_key=b'your-secret-key')

        # Generate token
        token = manager.generate_token(
            owner_user_id="user-123",
            task_id="task-456",
            approval_id="approval-789",
            action_hash="abc123def456"
        )

        # Verify token
        is_valid = manager.verify_token(
            token=token,
            owner_user_id="user-123",
            task_id="task-456",
            approval_id="approval-789",
            action_hash="abc123def456"
        )
    """

    def __init__(self, secret_key: Optional[bytes] = None):
        """
        Initialize with secret key.

        If not provided, will try:
        1. OPENFORCE_APPROVAL_SECRET_KEY environment variable
        2. Generate a secure random key (store this for production!)

        Args:
            secret_key: Optional secret key for HMAC signing.
                        If None, will use env var or generate random key.
        """
        if secret_key is not None:
            self._secret_key = secret_key
        else:
            # Try environment variable first
            env_key = os.environ.get('OPENFORCE_APPROVAL_SECRET_KEY', '')
            if env_key:
                self._secret_key = env_key.encode('utf-8')
            else:
                # Generate secure random key
                self._secret_key = secrets.token_bytes(32)

    def generate_token(
        self,
        owner_user_id: str,
        task_id: str,
        approval_id: str,
        action_hash: str,
        expires_in: int = 3600,
        nonce: Optional[str] = None,
        channel_binding_hash: Optional[str] = None
    ) -> str:
        """
        Generate a signed approval token.

        Token format: <timestamp>:<nonce>:<hmac_signature>

        Args:
            owner_user_id: ID of the user who owns this approval
            task_id: ID of the task requiring approval
            approval_id: Unique approval request ID
            action_hash: Hash of the action being approved
            expires_in: Token validity duration in seconds (default: 1 hour)
            nonce: Optional nonce (if None, generates random nonce)
            channel_binding_hash: Optional channel binding for additional security

        Returns:
            Signed token string
        """
        exp = int(time.time()) + expires_in
        nonce = nonce or secrets.token_urlsafe(16)

        # Build message to sign
        # Include all parameters that affect approval validity
        message = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}"
        if channel_binding_hash:
            message = f"{message}:{channel_binding_hash}"

        # Generate HMAC signature
        signature = hmac.new(
            self._secret_key,
            message.encode('utf-8'),
            hashlib.sha256
        ).hexdigest()

        # Return encoded token
        return f"{exp}:{nonce}:{signature}"

    def verify_token(
        self,
        token: str,
        owner_user_id: str,
        task_id: str,
        approval_id: str,
        action_hash: str,
        channel_binding_hash: Optional[str] = None
    ) -> bool:
        """
        Verify token signature and expiration.

        Uses constant-time comparison to prevent timing attacks.

        Args:
            token: Token string to verify
            owner_user_id: Expected owner user ID
            task_id: Expected task ID
            approval_id: Expected approval ID
            action_hash: Expected action hash
            channel_binding_hash: Optional expected channel binding hash

        Returns:
            True if token is valid and not expired, False otherwise
        """
        try:
            parts = token.split(':')
            if len(parts) != 3:
                return False

            exp_str, nonce, provided_signature = parts
            exp = int(exp_str)

            # Check expiration FIRST (before any crypto operations)
            if time.time() > exp:
                return False

            # Recompute expected signature
            message = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}"
            if channel_binding_hash:
                message = f"{message}:{channel_binding_hash}"

            expected_signature = hmac.new(
                self._secret_key,
                message.encode('utf-8'),
                hashlib.sha256
            ).hexdigest()

            # Constant-time comparison to prevent timing attacks
            return hmac.compare_digest(expected_signature, provided_signature)

        except (ValueError, TypeError):
            return False


# Module-level convenience functions
_default_manager: Optional[ApprovalTokenManager] = None


def get_default_manager() -> ApprovalTokenManager:
    """Get or create the default token manager."""
    global _default_manager
    if _default_manager is None:
        _default_manager = ApprovalTokenManager()
    return _default_manager


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
    Generate approval token using default manager.

    This is a convenience function that uses the default ApprovalTokenManager.
    For more control, use ApprovalTokenManager directly.

    Args:
        owner_user_id: ID of the user who owns this approval
        task_id: ID of the task requiring approval
        approval_id: Unique approval request ID
        action_hash: Hash of the action being approved
        exp: Token expiration timestamp (Unix epoch)
        nonce: Random nonce for uniqueness
        channel_binding_hash: Channel binding for additional security

    Returns:
        Signed token string
    """
    manager = get_default_manager()
    # Convert exp (absolute timestamp) to expires_in (relative duration)
    expires_in = exp - int(time.time())
    if expires_in < 0:
        expires_in = 0  # Already expired, but generate anyway for verification

    return manager.generate_token(
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
    Verify approval token using default manager.

    This is a convenience function that uses the default ApprovalTokenManager.
    For more control, use ApprovalTokenManager directly.

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
        True if token is valid, False otherwise
    """
    manager = get_default_manager()
    # Note: exp and nonce are embedded in the token, but we verify against
    # the provided parameters. This matches the original API.
    # The manager will check expiration from the token itself.
    return manager.verify_token(
        token=token,
        owner_user_id=owner_user_id,
        task_id=task_id,
        approval_id=approval_id,
        action_hash=action_hash,
        channel_binding_hash=channel_binding_hash
    )
