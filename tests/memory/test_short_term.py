"""Tests for MEM-02: Short-term memory."""
import pytest
from src.memory.short_term import ShortTermMemory, MemoryItem, SessionSummary


class TestMemoryItem:
    """Tests for MemoryItem dataclass."""

    def test_to_dict(self):
        """Test serialization to dictionary."""
        item = MemoryItem(content="test", role="user")
        d = item.to_dict()
        assert d["content"] == "test"
        assert d["role"] == "user"
        assert "timestamp" in d
        assert "metadata" in d

    def test_from_dict(self):
        """Test deserialization from dictionary."""
        item = MemoryItem.from_dict({"content": "test", "role": "assistant"})
        assert item.content == "test"
        assert item.role == "assistant"

    def test_from_dict_with_optional_fields(self):
        """Test deserialization with optional fields."""
        item = MemoryItem.from_dict({
            "content": "test",
            "role": "user",
            "timestamp": "2024-01-01T00:00:00",
            "metadata": {"key": "value"}
        })
        assert item.timestamp == "2024-01-01T00:00:00"
        assert item.metadata == {"key": "value"}


class TestSessionSummary:
    """Tests for SessionSummary dataclass."""

    def test_session_summary_creation(self):
        """Test creating a session summary."""
        summary = SessionSummary(
            start_time="2024-01-01T00:00:00",
            end_time="2024-01-01T01:00:00",
            message_count=10,
            summary_text="Test summary"
        )
        assert summary.message_count == 10
        assert summary.key_points == []


class TestShortTermMemory:
    """Tests for ShortTermMemory class."""

    def test_add_message(self):
        """Test adding a message."""
        memory = ShortTermMemory()
        memory.add_message("Hello", "user")
        assert memory.message_count == 1

    def test_add_message_with_metadata(self):
        """Test adding a message with metadata."""
        memory = ShortTermMemory()
        memory.add_message("Hello", "user", {"intent": "greeting"})
        messages = memory.get_all_messages()
        assert messages[0]["metadata"]["intent"] == "greeting"

    def test_get_recent_messages(self):
        """Test getting recent messages."""
        memory = ShortTermMemory()
        for i in range(5):
            memory.add_message(f"msg{i}", "user")
        recent = memory.get_recent_messages(3)
        assert len(recent) == 3
        assert recent[-1]["content"] == "msg4"

    def test_get_recent_messages_fewer_than_count(self):
        """Test getting more messages than exist."""
        memory = ShortTermMemory()
        memory.add_message("only one", "user")
        recent = memory.get_recent_messages(10)
        assert len(recent) == 1

    def test_get_all_messages(self):
        """Test getting all messages."""
        memory = ShortTermMemory()
        for i in range(3):
            memory.add_message(f"msg{i}", "user")
        all_msgs = memory.get_all_messages()
        assert len(all_msgs) == 3

    def test_search(self):
        """Test searching messages."""
        memory = ShortTermMemory()
        memory.add_message("python code", "user")
        memory.add_message("javascript code", "user")
        results = memory.search("python")
        assert len(results) == 1
        assert "python" in results[0]["content"]

    def test_search_case_insensitive(self):
        """Test case-insensitive search."""
        memory = ShortTermMemory()
        memory.add_message("Python Code", "user")
        results = memory.search("python")
        assert len(results) == 1

    def test_search_multiple_results(self):
        """Test search with multiple results."""
        memory = ShortTermMemory()
        memory.add_message("python test 1", "user")
        memory.add_message("python test 2", "user")
        memory.add_message("python test 3", "user")
        results = memory.search("python", limit=2)
        assert len(results) == 2

    def test_search_no_results(self):
        """Test search with no matches."""
        memory = ShortTermMemory()
        memory.add_message("hello world", "user")
        results = memory.search("python")
        assert len(results) == 0

    def test_compression(self):
        """Test automatic compression."""
        memory = ShortTermMemory(max_messages=50, compression_threshold=10)
        for i in range(15):
            memory.add_message(f"message {i}", "user")
        assert memory.message_count <= 20  # Should have compressed
        assert memory.summary_count >= 1

    def test_compression_preserves_recent(self):
        """Test that compression preserves recent messages."""
        memory = ShortTermMemory(max_messages=50, compression_threshold=10)
        for i in range(15):
            memory.add_message(f"message {i}", "user")
        recent = memory.get_recent_messages(1)
        assert recent[0]["content"] == "message 14"

    def test_context_set_and_get(self):
        """Test context storage."""
        memory = ShortTermMemory()
        memory.set_context("topic", "testing")
        assert memory.get_context("topic") == "testing"

    def test_context_default_value(self):
        """Test context default value."""
        memory = ShortTermMemory()
        assert memory.get_context("missing", "default") == "default"

    def test_context_none_default(self):
        """Test context with None default."""
        memory = ShortTermMemory()
        assert memory.get_context("missing") is None

    def test_clear(self):
        """Test clearing memory."""
        memory = ShortTermMemory()
        memory.add_message("test", "user")
        memory.set_context("key", "value")
        memory.clear()
        assert memory.message_count == 0
        assert memory.summary_count == 0
        assert memory.get_context("key") is None

    def test_to_json_and_from_json(self):
        """Test serialization and deserialization."""
        memory = ShortTermMemory()
        memory.add_message("test", "user")
        memory.set_context("topic", "testing")
        json_str = memory.to_json()
        restored = ShortTermMemory.from_json(json_str)
        assert restored.message_count == 1
        assert restored.get_context("topic") == "testing"

    def test_summary_count(self):
        """Test summary count property."""
        memory = ShortTermMemory()
        assert memory.summary_count == 0

    def test_multiple_compressions(self):
        """Test multiple compression cycles."""
        memory = ShortTermMemory(max_messages=100, compression_threshold=10)
        for i in range(35):
            memory.add_message(f"message {i}", "user")
        assert memory.summary_count >= 2

    def test_empty_memory_json(self):
        """Test JSON serialization of empty memory."""
        memory = ShortTermMemory()
        json_str = memory.to_json()
        restored = ShortTermMemory.from_json(json_str)
        assert restored.message_count == 0
