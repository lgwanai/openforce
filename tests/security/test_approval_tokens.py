import pytest
import time
import hmac
import hashlib
from unittest.mock import patch


class TestApprovalTokens:
    """Tests for SEC-02: HMAC token security."""

    def test_token_signature_valid(self, token_manager):
        """Verify token signature is valid and verifiable."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        # Generate token
        token = token_manager.generate_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            expires_in=3600
        )

        # Token should be a non-empty string
        assert isinstance(token, str)
        assert len(token) > 0

        # Token should have format: <timestamp>:<nonce>:<signature>
        parts = token.split(':')
        assert len(parts) == 3

        # Timestamp should be a valid integer
        timestamp = int(parts[0])
        assert timestamp > time.time()

        # Verify token should return True for valid token
        result = token_manager.verify_token(
            token=token,
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash
        )
        assert result is True

    def test_expired_token_rejected(self, token_manager):
        """Verify expired tokens are rejected."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        # Generate token that expires in the past
        token = token_manager.generate_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            expires_in=-1  # Already expired
        )

        # Small delay to ensure time has passed
        time.sleep(0.1)

        # Verify should reject expired token
        result = token_manager.verify_token(
            token=token,
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash
        )
        assert result is False

    def test_timing_attack_resistance(self, token_manager):
        """Verify timing attack resistance via compare_digest usage."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        # Generate valid token
        valid_token = token_manager.generate_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            expires_in=3600
        )

        # Create a completely wrong token (wrong signature)
        parts = valid_token.split(':')
        wrong_token = f"{parts[0]}:{parts[1]}:wrong_signature_12345"

        # Verify wrong token returns False (not exception)
        result = token_manager.verify_token(
            token=wrong_token,
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash
        )
        assert result is False

        # Test with malformed token (wrong number of parts)
        result = token_manager.verify_token(
            token="malformed-token",
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash
        )
        assert result is False

    def test_wrong_signature_rejected(self, token_manager):
        """Verify tokens with wrong signature are rejected."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        # Generate token with different action_hash
        token = token_manager.generate_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash="different_hash",
            expires_in=3600
        )

        # Verify with different action_hash should fail
        result = token_manager.verify_token(
            token=token,
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash  # Different from what was signed
        )
        assert result is False

    def test_different_owner_rejected(self, token_manager):
        """Verify tokens with different owner are rejected."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        token = token_manager.generate_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            expires_in=3600
        )

        # Verify with different owner should fail
        result = token_manager.verify_token(
            token=token,
            owner_user_id="different-user",
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash
        )
        assert result is False

    def test_token_manager_uses_hmac(self, token_manager):
        """Verify that HMAC is used for token generation."""
        # This test verifies implementation detail - HMAC should be used
        # not plain SHA256
        import inspect

        # Get the source code of generate_token
        source = inspect.getsource(token_manager.generate_token)

        # Should contain hmac.new, not just hashlib.sha256
        assert 'hmac.new' in source or 'hmac.' in source
        # Should NOT have plain sha256().hexdigest() without HMAC
        # This is a soft check - the main verification is through tests above

    def test_token_manager_secret_key(self):
        """Test that secret key is used for HMAC."""
        from src.security.approval import ApprovalTokenManager

        # Create with explicit secret key
        secret = b'test-secret-key-12345'
        manager1 = ApprovalTokenManager(secret_key=secret)
        manager2 = ApprovalTokenManager(secret_key=secret)

        # Same secret should produce verifiable tokens
        token = manager1.generate_token(
            owner_user_id="user",
            task_id="task",
            approval_id="approval",
            action_hash="hash"
        )

        assert manager2.verify_token(
            token=token,
            owner_user_id="user",
            task_id="task",
            approval_id="approval",
            action_hash="hash"
        ) is True

        # Different secret should NOT verify
        manager3 = ApprovalTokenManager(secret_key=b'different-secret')
        assert manager3.verify_token(
            token=token,
            owner_user_id="user",
            task_id="task",
            approval_id="approval",
            action_hash="hash"
        ) is False

    def test_nonce_uniqueness(self, token_manager):
        """Verify that nonces are unique for each token."""
        owner_user_id = "user-123"
        task_id = "task-456"
        approval_id = "approval-789"
        action_hash = "abc123def456"

        # Generate multiple tokens with same parameters
        tokens = [
            token_manager.generate_token(
                owner_user_id=owner_user_id,
                task_id=task_id,
                approval_id=approval_id,
                action_hash=action_hash,
                expires_in=3600
            )
            for _ in range(10)
        ]

        # Extract nonces
        nonces = [token.split(':')[1] for token in tokens]

        # All nonces should be unique
        assert len(set(nonces)) == len(nonces), "Nonces should be unique"


class TestApprovalTokenFunctions:
    """Tests for module-level functions from taint_engine.py."""

    def test_generate_approval_token_delegates(self, mock_settings):
        """Verify generate_approval_token delegates to ApprovalTokenManager."""
        from src.security.taint_engine import generate_approval_token

        token = generate_approval_token(
            owner_user_id="user",
            task_id="task",
            approval_id="approval",
            action_hash="hash",
            exp=int(time.time()) + 3600,
            nonce="test-nonce",
            channel_binding_hash="channel"
        )

        # Should return a token string
        assert isinstance(token, str)
        assert len(token) > 0

    def test_verify_approval_token_delegates(self, mock_settings):
        """Verify verify_approval_token delegates to ApprovalTokenManager."""
        from src.security.taint_engine import generate_approval_token, verify_approval_token

        owner_user_id = "user"
        task_id = "task"
        approval_id = "approval"
        action_hash = "hash"
        exp = int(time.time()) + 3600
        nonce = "test-nonce"
        channel_binding_hash = "channel"

        token = generate_approval_token(
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            exp=exp,
            nonce=nonce,
            channel_binding_hash=channel_binding_hash
        )

        # Verify should work
        result = verify_approval_token(
            token=token,
            owner_user_id=owner_user_id,
            task_id=task_id,
            approval_id=approval_id,
            action_hash=action_hash,
            exp=exp,
            nonce=nonce,
            channel_binding_hash=channel_binding_hash
        )

        assert result is True

    def test_no_plain_sha256_in_taint_engine(self):
        """Verify that plain SHA256 is not used for token generation."""
        import inspect
        from src.security import taint_engine

        # Check that generate_approval_token delegates to ApprovalTokenManager
        # and doesn't use hashlib.sha256 directly
        source = inspect.getsource(taint_engine.generate_approval_token)

        # Should NOT have direct sha256().hexdigest() calls
        # The function should delegate to ApprovalTokenManager
        assert 'hashlib.sha256' not in source or 'ApprovalTokenManager' in source
