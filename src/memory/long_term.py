"""
Long-term memory with knowledge graph storage.

Implements MEM-03: Knowledge graph storage and semantic retrieval
"""

import sqlite3
import json
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field
from datetime import datetime
import logging

logger = logging.getLogger(__name__)


@dataclass
class KnowledgeNode:
    """A node in the knowledge graph."""
    node_id: str
    node_type: str
    content: str
    embedding: List[float] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: str = field(default_factory=lambda: datetime.utcnow().isoformat())
    updated_at: str = field(default_factory=lambda: datetime.utcnow().isoformat())
    access_count: int = 0
    importance: float = 0.5


@dataclass
class KnowledgeEdge:
    """An edge connecting knowledge nodes."""
    source_id: str
    target_id: str
    relation: str
    weight: float = 1.0
    metadata: Dict[str, Any] = field(default_factory=dict)


class LongTermMemory:
    """Long-term memory using knowledge graph."""

    def __init__(self, db_path: str = ":memory:"):
        self.db_path = db_path
        self._nodes: Dict[str, KnowledgeNode] = {}
        self._edges: List[KnowledgeEdge] = []
        self._conn: Optional[sqlite3.Connection] = None
        self._init_db()

    def _get_connection(self) -> sqlite3.Connection:
        """Get or create a database connection."""
        if self._conn is None:
            self._conn = sqlite3.connect(self.db_path)
        return self._conn

    def _init_db(self) -> None:
        conn = self._get_connection()
        c = conn.cursor()
        c.execute('''CREATE TABLE IF NOT EXISTS knowledge_nodes (node_id TEXT PRIMARY KEY, node_type TEXT, content TEXT, embedding TEXT, metadata TEXT, created_at TEXT, updated_at TEXT, access_count INTEGER, importance REAL)''')
        c.execute('''CREATE TABLE IF NOT EXISTS knowledge_edges (id INTEGER PRIMARY KEY AUTOINCREMENT, source_id TEXT, target_id TEXT, relation TEXT, weight REAL, metadata TEXT)''')
        conn.commit()

    def add_node(self, node_id: str, node_type: str, content: str, embedding: List[float] = None, metadata: Dict[str, Any] = None, importance: float = 0.5) -> str:
        node = KnowledgeNode(node_id=node_id, node_type=node_type, content=content, embedding=embedding or [], metadata=metadata or {}, importance=importance)
        self._nodes[node_id] = node
        self._save_node(node)
        return f"Added node {node_id}"

    def add_edge(self, source_id: str, target_id: str, relation: str, weight: float = 1.0, metadata: Dict[str, Any] = None) -> str:
        edge = KnowledgeEdge(source_id=source_id, target_id=target_id, relation=relation, weight=weight, metadata=metadata or {})
        self._edges.append(edge)
        self._save_edge(edge)
        return f"Added edge {source_id} -> {target_id}"

    def get_node(self, node_id: str) -> Optional[Dict[str, Any]]:
        if node_id in self._nodes:
            node = self._nodes[node_id]
            node.access_count += 1
            return {"node_id": node.node_id, "node_type": node.node_type, "content": node.content, "metadata": node.metadata, "importance": node.importance, "access_count": node.access_count}
        return None

    def search(self, query: str, node_type: str = None, limit: int = 10) -> List[Dict[str, Any]]:
        results = []
        query_lower = query.lower()
        for node in self._nodes.values():
            if query_lower in node.content.lower():
                if node_type is None or node.node_type == node_type:
                    results.append({"node_id": node.node_id, "node_type": node.node_type, "content": node.content, "importance": node.importance})
        results.sort(key=lambda x: x["importance"], reverse=True)
        return results[:limit]

    def get_related(self, node_id: str, relation: str = None, max_depth: int = 2) -> List[Dict[str, Any]]:
        related = []
        visited = {node_id}
        def traverse(current_id: str, depth: int):
            if depth > max_depth: return
            for edge in self._edges:
                next_id = None
                if edge.source_id == current_id and edge.target_id not in visited:
                    if relation is None or edge.relation == relation: next_id = edge.target_id
                elif edge.target_id == current_id and edge.source_id not in visited:
                    if relation is None or edge.relation == relation: next_id = edge.source_id
                if next_id and next_id not in visited:
                    visited.add(next_id)
                    node = self.get_node(next_id)
                    if node: related.append({**node, "relation": edge.relation, "distance": depth})
                    traverse(next_id, depth + 1)
        traverse(node_id, 1)
        return related

    def update_importance(self, node_id: str, delta: float) -> None:
        if node_id in self._nodes:
            self._nodes[node_id].importance = min(1.0, max(0.0, self._nodes[node_id].importance + delta))

    def _save_node(self, node: KnowledgeNode) -> None:
        conn = self._get_connection()
        c = conn.cursor()
        c.execute('INSERT OR REPLACE INTO knowledge_nodes (node_id, node_type, content, embedding, metadata, created_at, updated_at, access_count, importance) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
                  (node.node_id, node.node_type, node.content, json.dumps(node.embedding), json.dumps(node.metadata), node.created_at, node.updated_at, node.access_count, node.importance))
        conn.commit()

    def _save_edge(self, edge: KnowledgeEdge) -> None:
        conn = self._get_connection()
        c = conn.cursor()
        c.execute('INSERT INTO knowledge_edges (source_id, target_id, relation, weight, metadata) VALUES (?, ?, ?, ?, ?)', (edge.source_id, edge.target_id, edge.relation, edge.weight, json.dumps(edge.metadata)))
        conn.commit()

    @property
    def node_count(self) -> int: return len(self._nodes)

    @property
    def edge_count(self) -> int: return len(self._edges)
