"""Tests for MEM-03: Long-term memory."""
import pytest
from src.memory.long_term import LongTermMemory, KnowledgeNode, KnowledgeEdge


class TestKnowledgeNode:
    def test_creation(self):
        node = KnowledgeNode(node_id="test", node_type="fact", content="test content")
        assert node.node_id == "test"
        assert node.importance == 0.5


class TestLongTermMemory:
    def test_add_node(self):
        memory = LongTermMemory()
        result = memory.add_node("node1", "fact", "Python is a language")
        assert "node1" in result
        assert memory.node_count == 1

    def test_get_node(self):
        memory = LongTermMemory()
        memory.add_node("node1", "fact", "test content")
        node = memory.get_node("node1")
        assert node["content"] == "test content"
        assert node["access_count"] == 1

    def test_get_nonexistent_node(self):
        memory = LongTermMemory()
        assert memory.get_node("nonexistent") is None

    def test_add_edge(self):
        memory = LongTermMemory()
        memory.add_node("n1", "fact", "a")
        memory.add_node("n2", "fact", "b")
        result = memory.add_edge("n1", "n2", "related_to")
        assert "n1" in result
        assert memory.edge_count == 1

    def test_search(self):
        memory = LongTermMemory()
        memory.add_node("n1", "fact", "Python programming", importance=0.8)
        memory.add_node("n2", "fact", "JavaScript code", importance=0.6)
        results = memory.search("Python")
        assert len(results) == 1
        assert results[0]["node_id"] == "n1"

    def test_search_by_type(self):
        memory = LongTermMemory()
        memory.add_node("n1", "fact", "test fact")
        memory.add_node("n2", "entity", "test entity")
        results = memory.search("test", node_type="fact")
        assert len(results) == 1
        assert results[0]["node_type"] == "fact"

    def test_get_related(self):
        memory = LongTermMemory()
        memory.add_node("n1", "fact", "a")
        memory.add_node("n2", "fact", "b")
        memory.add_edge("n1", "n2", "related_to")
        related = memory.get_related("n1")
        assert len(related) == 1
        assert related[0]["node_id"] == "n2"

    def test_update_importance(self):
        memory = LongTermMemory()
        memory.add_node("n1", "fact", "test", importance=0.5)
        memory.update_importance("n1", 0.3)
        node = memory.get_node("n1")
        assert node["importance"] == 0.8
