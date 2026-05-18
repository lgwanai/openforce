"""User context module - simplified version for single-user skill.

The full deer-flow implementation supports multi-user isolation via contextvars.
This simplified version always returns 'default' as the effective user ID.
"""

from typing import Optional

DEFAULT_USER_ID = "default"
AUTO = object()


def get_effective_user_id() -> str:
    """Return the default user ID for single-user skill mode."""
    return DEFAULT_USER_ID


def resolve_runtime_user_id(
    user_id: Optional[str] = None,
    *,
    raise_if_unset: bool = True,
) -> str:
    """Resolve user_id for runtime paths - always returns default."""
    return user_id or DEFAULT_USER_ID
