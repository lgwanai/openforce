"""Tests for MEM-05: Memory retrieval."""
import pytest
from datetime import datetime, timedelta
from src.memory.retrieval import MemoryRetriever, RetrievalResult, RetrievalConfig
from src.memory.short_term import ShortTermMemory
from src.memory.long_term import LongTermMemory


class TestRetrievalConfig:
    def test_defaults(self):
        config = RetrievalConfig()
        assert config.importance_weight == 0.4
        assert config.recency_weight == 0.3
        assert config.relevance_weight == 0.2
        assert config.access_weight == 0.1
        assert config.time_decay_hours == 24.0

    def test_custom_weights(self):
        config = RetrievalConfig(
            importance_weight=0.5,
            recency_weight=0.3,
            relevance_weight=0.1,
            access_weight=0.1
        )
        assert config.importance_weight == 0.5


class TestRetrievalResult:
    def test_creation(self):
        result = RetrievalResult(
            item={"content": "test"},
            score=0.8,
            components={"importance": 0.5}
        )
        assert result.score == 0.8
        assert result.item["content"] == "test"
        assert result.components["importance"] == 0.5


class TestMemoryRetriever:
    def test_retrieve_empty(self):
        retriever = MemoryRetriever()
        results = retriever.retrieve("test")
        assert len(results) == 0

    def test_retrieve_from_short_term(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python programming guide", "user")
        short_term.add_message("JavaScript tutorial", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        results = retriever.retrieve("Python")

        assert len(results) == 1
        assert "Python" in results[0].item["content"]

    def test_retrieve_from_long_term(self):
        long_term = LongTermMemory()
        long_term.add_node("n1", "fact", "Python is a language", importance=0.9)

        retriever = MemoryRetriever(long_term_memory=long_term)
        results = retriever.retrieve("Python")

        assert len(results) == 1

    def test_retrieve_from_both_memories(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python basics", "user")

        long_term = LongTermMemory()
        long_term.add_node("n1", "fact", "Python advanced topics", importance=0.8)

        retriever = MemoryRetriever(
            short_term_memory=short_term,
            long_term_memory=long_term
        )
        results = retriever.retrieve("Python")

        assert len(results) == 2

    def test_score_calculation(self):
        short_term = ShortTermMemory()
        short_term.add_message("test query match", "user")
        retriever = MemoryRetriever(short_term_memory=short_term)

        results = retriever.retrieve("query")
        assert len(results) == 1
        assert results[0].score > 0
        assert "importance" in results[0].components
        assert "recency" in results[0].components
        assert "relevance" in results[0].components
        assert "access" in results[0].components

    def test_retrieve_by_importance(self):
        long_term = LongTermMemory()
        long_term.add_node("n1", "fact", "high importance", importance=0.9)
        long_term.add_node("n2", "fact", "low importance", importance=0.3)

        retriever = MemoryRetriever(long_term_memory=long_term)
        results = retriever.retrieve_by_importance(min_importance=0.5)

        assert len(results) == 1
        assert results[0].item["importance"] == 0.9

    def test_retrieve_by_importance_all(self):
        long_term = LongTermMemory()
        long_term.add_node("n1", "fact", "first", importance=0.9)
        long_term.add_node("n2", "fact", "second", importance=0.7)

        retriever = MemoryRetriever(long_term_memory=long_term)
        results = retriever.retrieve_by_importance()

        assert len(results) == 2
        assert results[0].item["importance"] >= results[1].item["importance"]

    def test_retrieve_recent(self):
        short_term = ShortTermMemory()
        short_term.add_message("recent message", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        results = retriever.retrieve_recent(hours=24)

        assert len(results) >= 1

    def test_retrieve_recent_with_old_messages(self):
        short_term = ShortTermMemory()

        # Add a message and manually set old timestamp
        short_term.add_message("old message", "user")
        # Modify timestamp to be old
        old_time = (datetime.utcnow() - timedelta(hours=48)).isoformat()
        short_term._messages[0].timestamp = old_time

        short_term.add_message("recent message", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        results = retriever.retrieve_recent(hours=24)

        assert len(results) == 1
        assert "recent" in results[0].item["content"]

    def test_get_context_for_query(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python is a programming language", "user")
        short_term.add_message("It is used for web development", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        context = retriever.get_context_for_query("Python")

        assert "Python" in context

    def test_get_context_respects_token_limit(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python is great", "user")
        short_term.add_message("JavaScript is also great", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        # Use very small token limit
        context = retriever.get_context_for_query("great", max_tokens=3)

        # Should only include first result
        assert "Python" in context or "JavaScript" in context

    def test_recency_score_decay(self):
        retriever = MemoryRetriever()

        # Recent timestamp
        recent = datetime.utcnow().isoformat()
        assert retriever._calculate_recency_score(recent) > 0.9

        # Old timestamp
        old = (datetime.utcnow() - timedelta(hours=48)).isoformat()
        assert retriever._calculate_recency_score(old) < 0.5

    def test_recency_score_empty_timestamp(self):
        retriever = MemoryRetriever()
        assert retriever._calculate_recency_score("") == 0.0

    def test_recency_score_invalid_timestamp(self):
        retriever = MemoryRetriever()
        # Invalid timestamp should return default
        assert retriever._calculate_recency_score("invalid") == 0.5

    def test_relevance_score(self):
        retriever = MemoryRetriever()

        # High relevance
        score = retriever._calculate_relevance_score("python code", "python programming code")
        assert score > 0.5

        # No overlap
        score = retriever._calculate_relevance_score("python", "javascript")
        assert score == 0.0

    def test_relevance_score_empty(self):
        retriever = MemoryRetriever()

        assert retriever._calculate_relevance_score("", "content") == 0.0
        assert retriever._calculate_relevance_score("query", "") == 0.0
        assert retriever._calculate_relevance_score("", "") == 0.0

    def test_custom_config(self):
        config = RetrievalConfig(
            importance_weight=0.5,
            recency_weight=0.2,
            relevance_weight=0.2,
            access_weight=0.1
        )
        retriever = MemoryRetriever(config=config)

        assert retriever.config.importance_weight == 0.5

    def test_memory_types_filter(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python basics", "user")

        long_term = LongTermMemory()
        long_term.add_node("n1", "fact", "Python advanced", importance=0.8)

        retriever = MemoryRetriever(
            short_term_memory=short_term,
            long_term_memory=long_term
        )

        # Only short_term
        results = retriever.retrieve("Python", memory_types=["short_term"])
        assert len(results) == 1

        # Only long_term
        results = retriever.retrieve("Python", memory_types=["long_term"])
        assert len(results) == 1

    def test_limit_parameter(self):
        short_term = ShortTermMemory()
        for i in range(10):
            short_term.add_message(f"Python message {i}", "user")

        retriever = MemoryRetriever(short_term_memory=short_term)
        results = retriever.retrieve("Python", limit=3)

        assert len(results) == 3
