"""Tests for MEM-01: User info collector."""
import pytest
from src.tools.user_info_collector import (
    UserInfoCollector, UserProfile, ConversationContext,
    tool_collect_user_info, tool_set_preference, tool_get_profile,
    _collector_instance, _get_collector
)


@pytest.fixture(autouse=True)
def reset_collector_singleton():
    """Reset the singleton collector before each test."""
    import src.tools.user_info_collector as module
    module._collector_instance = None
    yield
    module._collector_instance = None


class TestUserProfile:
    def test_profile_creation(self):
        profile = UserProfile(user_id="user1", name="Alice")
        assert profile.user_id == "user1"
        assert profile.name == "Alice"

class TestUserInfoCollector:
    def test_collect_basic_info(self):
        collector = UserInfoCollector()
        result = collector.collect_basic_info("user1", name="Alice", email="alice@example.com")
        assert "user1" in result
        profile = collector.get_profile("user1")
        assert profile["name"] == "Alice"

    def test_set_preference(self):
        collector = UserInfoCollector()
        collector.set_preference("user1", "theme", "dark")
        profile = collector.get_profile("user1")
        assert profile["preferences"]["theme"] == "dark"

    def test_get_nonexistent_profile(self):
        collector = UserInfoCollector()
        assert collector.get_profile("nonexistent") is None

    def test_create_context(self):
        collector = UserInfoCollector()
        result = collector.create_context("session1", topic="coding")
        assert "session1" in result
        ctx = collector.get_context("session1")
        assert ctx["topic"] == "coding"

    def test_add_entity(self):
        collector = UserInfoCollector()
        collector.create_context("session1")
        collector.add_entity("session1", "project", {"name": "OpenForce"})
        ctx = collector.get_context("session1")
        assert ctx["entities"]["project"]["name"] == "OpenForce"

    def test_update_sentiment(self):
        collector = UserInfoCollector()
        collector.create_context("session1")
        collector.update_sentiment("session1", "positive")
        ctx = collector.get_context("session1")
        assert ctx["sentiment"] == "positive"

class TestToolFunctions:
    def test_tool_collect_user_info(self):
        result = tool_collect_user_info("user1", name="Bob")
        assert "user1" in result

    def test_tool_set_preference(self):
        result = tool_set_preference("user1", "lang", "en")
        assert "lang" in result

    def test_tool_get_profile(self):
        tool_collect_user_info("user1", name="Test")
        result = tool_get_profile("user1")
        assert "Test" in result

    def test_tool_get_nonexistent_profile(self):
        result = tool_get_profile("nonexistent")
        assert "not found" in result
