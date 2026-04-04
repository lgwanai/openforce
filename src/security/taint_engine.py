"""
Taint propagation and sensitive tool verification protocol.

This module provides taint tracking for security-sensitive operations:
- TrustLevel: TRUSTED | DERIVED | UNTRUSTED
- TaintSource: INTERNAL | WEB | SEARCH | UPLOAD | USER_FREE_TEXT

For approval token generation and verification, see src.security.approval module.
"""

import os
import time
from dataclasses import dataclass, field
from enum import Enum
from functools import wraps
from typing import Any, Dict, List, Optional, Set

from .approval import ApprovalTokenManager


class TrustLevel(Enum):
    """Trust levels for data provenance."""
    TRUSTED = "trusted"
    DERIVED = "derived"
    UNTRUSTED = "untrusted"


class TaintSource(Enum):
    """Sources of potentially tainted data."""
    INTERNAL = "internal"
    USER_FREE_TEXT = "user_free_text"
    WEB = "web"
    SEARCH = "search"
    UPLOAD = "upload"


@dataclass
class TaintedValue:
    """
    A value with taint metadata.

    Tracks the source(s) of data and derived trust level for security decisions.
    """
    value: Any
    sources: Set[TaintSource] = field(default_factory=lambda: {TaintSource.INTERNAL})
    trust_level: TrustLevel = TrustLevel.TRUSTED

    def __post_init__(self):
        """Derive trust level from sources after initialization."""
        if TaintSource.WEB in self.sources or TaintSource.UPLOAD in self.sources:
            self.trust_level = TrustLevel.UNTRUSTED
        elif TaintSource.USER_FREE_TEXT in self.sources or TaintSource.SEARCH in self.sources:
            self.trust_level = TrustLevel.DERIVED

    def propagate_to(self, new_value: Any) -> 'TaintedValue':
        """Create a new TaintedValue with the same taint metadata."""
        return TaintedValue(
            value=new_value,
            sources=self.sources.copy(),
            trust_level=self.trust_level
        )

    @staticmethod
    def trusted(value: Any) -> 'TaintedValue':
        """Create a trusted value from internal sources."""
        return TaintedValue(value=value, sources={TaintSource.INTERNAL})

    @staticmethod
    def from_web(value: Any) -> 'TaintedValue':
        """Create a tainted value from web source."""
        return TaintedValue(value=value, sources={TaintSource.WEB})

    @staticmethod
    def from_user(value: Any) -> 'TaintedValue':
        """Create a tainted value from user input."""
        return TaintedValue(value=value, sources={TaintSource.USER_FREE_TEXT})


class TaintEngine:
    """
    Taint propagation and sensitive tool verification.

    Enforces trust-based access control for dangerous operations.
    """

    HIGH_RISK_TOOLS = frozenset([
        "execute_command",
        "delete_file",
        "write_api",
        "run_shell",
    ])

    MEDIUM_RISK_TOOLS = frozenset([
        "write_file",
        "run_script",
    ])

    @classmethod
    def check_tool_call(
        cls,
        tool_name: str,
        args: Dict[str, Any],
        tainted_args: Optional[Dict[str, TaintedValue]] = None
    ) -> bool:
        """
        Check if tool call is allowed based on taint level.

        High-risk tools always require human-in-the-loop approval.
        Medium-risk tools check if arguments contain untrusted data.

        Args:
            tool_name: Name of the tool being called
            args: Tool arguments (raw values)
            tainted_args: Optional mapping of argument names to TaintedValue objects

        Returns:
            True if the tool call is allowed, False if it should be blocked
        """
        # High-risk tools always require approval
        if tool_name in cls.HIGH_RISK_TOOLS:
            return False  # Force human-in-the-loop

        # Check taint for medium-risk tools
        if tool_name in cls.MEDIUM_RISK_TOOLS:
            if tainted_args:
                for arg_name, tainted_value in tainted_args.items():
                    if tainted_value.trust_level == TrustLevel.UNTRUSTED:
                        return False  # Block untrusted data

        return True

    @staticmethod
    def get_trust_level(sources: List[TaintSource]) -> TrustLevel:
        """
        Derive trust level from a list of sources.

        Args:
            sources: List of TaintSource values

        Returns:
            The derived TrustLevel based on the most restrictive source
        """
        source_set = set(sources)
        if TaintSource.WEB in source_set or TaintSource.UPLOAD in source_set:
            return TrustLevel.UNTRUSTED
        if TaintSource.USER_FREE_TEXT in source_set or TaintSource.SEARCH in source_set:
            return TrustLevel.DERIVED
        return TrustLevel.TRUSTED

    @staticmethod
    def sanitize(value: TaintedValue, sanitizer: str) -> TaintedValue:
        """
        Sanitize a tainted value and upgrade trust level.

        After sanitization, the value is marked as DERIVED (not fully TRUSTED
        since the original source was untrusted).

        Args:
            value: The TaintedValue to sanitize
            sanitizer: Name of the sanitizer used (for audit purposes)

        Returns:
            A new TaintedValue with upgraded trust level
        """
        # Create the new TaintedValue, but we need to bypass __post_init__
        # because it would override our trust_level based on sources
        result = TaintedValue.__new__(TaintedValue)
        result.value = value.value
        result.sources = value.sources | {TaintSource.INTERNAL}
        result.trust_level = TrustLevel.DERIVED
        return result


def taint_source(source: TaintSource):
    """
    Decorator to mark function output as tainted.

    Usage:
        @taint_source(TaintSource.WEB)
        def fetch_webpage(url: str) -> str:
            ...
    """
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            result = func(*args, **kwargs)
            # If already a TaintedValue, preserve it
            if isinstance(result, TaintedValue):
                return result
            # Otherwise wrap in new TaintedValue
            return TaintedValue(value=result, sources={source})
        return wrapper
    return decorator


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
