"""
User information collection tools.

Implements MEM-01: User info collection as independent tool
"""

import json
from typing import Dict, Any, List, Optional
from dataclasses import dataclass, field, asdict
from datetime import datetime
import logging

logger = logging.getLogger(__name__)


@dataclass
class UserProfile:
    """User profile information."""
    user_id: str
    name: Optional[str] = None
    email: Optional[str] = None
    timezone: Optional[str] = None
    language: str = "zh-CN"
    preferences: Dict[str, Any] = field(default_factory=dict)
    created_at: str = field(default_factory=lambda: datetime.utcnow().isoformat())
    updated_at: str = field(default_factory=lambda: datetime.utcnow().isoformat())


@dataclass
class ConversationContext:
    """Context for current conversation."""
    session_id: str
    topic: Optional[str] = None
    goals: List[str] = field(default_factory=list)
    entities: Dict[str, Any] = field(default_factory=dict)
    sentiment: str = "neutral"
    created_at: str = field(default_factory=lambda: datetime.utcnow().isoformat())


# Singleton instance for tool functions
_collector_instance: Optional["UserInfoCollector"] = None


def _get_collector() -> "UserInfoCollector":
    """Get or create the singleton collector instance."""
    global _collector_instance
    if _collector_instance is None:
        _collector_instance = UserInfoCollector()
    return _collector_instance


class UserInfoCollector:
    """Collects and manages user information."""

    def __init__(self):
        self._profiles: Dict[str, UserProfile] = {}
        self._contexts: Dict[str, ConversationContext] = {}

    def collect_basic_info(self, user_id: str, name: str = None, email: str = None, timezone: str = None) -> str:
        if user_id not in self._profiles:
            self._profiles[user_id] = UserProfile(user_id=user_id)
        profile = self._profiles[user_id]
        if name: profile.name = name
        if email: profile.email = email
        if timezone: profile.timezone = timezone
        profile.updated_at = datetime.utcnow().isoformat()
        return f"Collected basic info for {user_id}: name={name}, email={email}"

    def set_preference(self, user_id: str, key: str, value: Any) -> str:
        if user_id not in self._profiles:
            self._profiles[user_id] = UserProfile(user_id=user_id)
        self._profiles[user_id].preferences[key] = value
        self._profiles[user_id].updated_at = datetime.utcnow().isoformat()
        return f"Set preference {key}={value} for user {user_id}"

    def get_profile(self, user_id: str) -> Optional[Dict[str, Any]]:
        if user_id in self._profiles:
            return asdict(self._profiles[user_id])
        return None

    def create_context(self, session_id: str, topic: str = None, goals: List[str] = None) -> str:
        context = ConversationContext(session_id=session_id, topic=topic, goals=goals or [])
        self._contexts[session_id] = context
        return f"Created context for session {session_id}"

    def add_entity(self, session_id: str, entity_type: str, entity_data: Dict[str, Any]) -> str:
        if session_id not in self._contexts:
            self._contexts[session_id] = ConversationContext(session_id=session_id)
        self._contexts[session_id].entities[entity_type] = entity_data
        return f"Added entity {entity_type} to context"

    def get_context(self, session_id: str) -> Optional[Dict[str, Any]]:
        if session_id in self._contexts:
            return asdict(self._contexts[session_id])
        return None

    def update_sentiment(self, session_id: str, sentiment: str) -> str:
        if session_id in self._contexts:
            self._contexts[session_id].sentiment = sentiment
        return f"Updated sentiment to {sentiment}"


def tool_collect_user_info(user_id: str, name: str = None, email: str = None) -> str:
    """Tool function to collect basic user information."""
    collector = _get_collector()
    return collector.collect_basic_info(user_id, name, email)


def tool_set_preference(user_id: str, key: str, value: str) -> str:
    """Tool function to set a user preference."""
    collector = _get_collector()
    return collector.set_preference(user_id, key, value)


def tool_get_profile(user_id: str) -> str:
    """Tool function to get user profile as JSON."""
    collector = _get_collector()
    profile = collector.get_profile(user_id)
    return json.dumps(profile, ensure_ascii=False) if profile else "Profile not found"


def tool_create_context(session_id: str, topic: str = None) -> str:
    """Tool function to create a conversation context."""
    collector = _get_collector()
    return collector.create_context(session_id, topic)
