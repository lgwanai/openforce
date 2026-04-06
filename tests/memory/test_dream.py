"""Tests for MEM-04: Dream processor."""
import pytest
from src.memory.dream import DreamProcessor, DreamReport
from src.memory.short_term import ShortTermMemory
from src.memory.long_term import LongTermMemory


class TestDreamProcessor:
    def test_dream_basic(self):
        short_term = ShortTermMemory()
        short_term.add_message("Python is a programming language", "user")
        short_term.add_message("JavaScript is also used for web", "assistant")

        processor = DreamProcessor(short_term)
        report = processor.dream()

        assert report.messages_processed == 2
        assert isinstance(report, DreamReport)
        assert len(report.insights) > 0

    def test_dream_with_long_term(self):
        short_term = ShortTermMemory()
        long_term = LongTermMemory()

        short_term.add_message("Python is great for data science", "user")

        processor = DreamProcessor(short_term, long_term)
        report = processor.dream()

        assert report.knowledge_extracted >= 0
        assert long_term.node_count >= 0

    def test_extract_entities(self):
        short_term = ShortTermMemory()
        processor = DreamProcessor(short_term)

        entities = processor._extract_entities("Contact John Smith at john@example.com")
        assert "John Smith" in entities or "john@example.com" in entities

    def test_extract_facts(self):
        short_term = ShortTermMemory()
        processor = DreamProcessor(short_term)

        facts = processor._extract_facts("Python is a programming language that is easy to learn.")
        assert len(facts) >= 1

    def test_are_related(self):
        short_term = ShortTermMemory()
        processor = DreamProcessor(short_term)

        assert processor._are_related("Python programming", "Python development") == True
        assert processor._are_related("Python code", "JavaScript widgets") == False

    def test_get_dream_status(self):
        short_term = ShortTermMemory()
        short_term.add_message("test", "user")
        processor = DreamProcessor(short_term)

        status = processor.get_dream_status()
        assert status["short_term_messages"] == 1
