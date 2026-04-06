"""
Short-term memory management.

Implements MEM-02: Session context management with compression
"""

from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field
from datetime import datetime
import json
import logging

logger = logging.getLogger(__name__)


@dataclass
class MemoryItem:
    """Single memory item."""
    content: str
    role: str
    timestamp: str = field(default_factory=lambda: datetime.utcnow().isoformat())
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "content": self.content,
            "role": self.role,
            "timestamp": self.timestamp,
            "metadata": self.metadata
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'MemoryItem':
        return cls(
            content=data["content"],
            role=data["role"],
            timestamp=data.get("timestamp", ""),
            metadata=data.get("metadata", {})
        )


@dataclass
class SessionSummary:
    """Summary of a session segment."""
    start_time: str
    end_time: str
    message_count: int
    summary_text: str
    key_points: List[str] = field(default_factory=list)


class ShortTermMemory:
    """Short-term memory with sliding window and compression."""

    def __init__(
        self,
        max_messages: int = 50,
        compression_threshold: int = 40,
        summary_max_length: int = 500
    ):
        self.max_messages = max_messages
        self.compression_threshold = compression_threshold
        self.summary_max_length = summary_max_length
        self._messages: List[MemoryItem] = []
        self._summaries: List[SessionSummary] = []
        self._context: Dict[str, Any] = {}

    def add_message(
        self,
        content: str,
        role: str,
        metadata: Dict[str, Any] = None
    ) -> None:
        """Add a message to memory."""
        item = MemoryItem(content=content, role=role, metadata=metadata or {})
        self._messages.append(item)
        if len(self._messages) >= self.compression_threshold:
            self._compress()

    def get_recent_messages(self, count: int = 10) -> List[Dict[str, Any]]:
        """Get the most recent messages."""
        return [m.to_dict() for m in self._messages[-count:]]

    def get_all_messages(self) -> List[Dict[str, Any]]:
        """Get all messages."""
        return [m.to_dict() for m in self._messages]

    def search(self, query: str, limit: int = 5) -> List[Dict[str, Any]]:
        """Search for messages containing query."""
        results = []
        query_lower = query.lower()
        for item in reversed(self._messages):
            if query_lower in item.content.lower():
                results.append(item.to_dict())
                if len(results) >= limit:
                    break
        return results

    def _compress(self) -> None:
        """Compress old messages into a summary."""
        if len(self._messages) < self.compression_threshold:
            return
        to_summarize = self._messages[:-20]
        self._messages = self._messages[-20:]
        summary = self._create_summary(to_summarize)
        self._summaries.append(summary)
        logger.info(f"Compressed {len(to_summarize)} messages")

    def _create_summary(self, messages: List[MemoryItem]) -> SessionSummary:
        """Create a summary from messages."""
        start_time = messages[0].timestamp if messages else ""
        end_time = messages[-1].timestamp if messages else ""
        key_points = [
            m.content[:100]
            for m in messages
            if m.role == "user" and len(m.content) > 10
        ]
        summary_text = f"Session segment with {len(messages)} messages. Key topics: {'; '.join(key_points[:5])}"
        return SessionSummary(
            start_time=start_time,
            end_time=end_time,
            message_count=len(messages),
            summary_text=summary_text[:self.summary_max_length],
            key_points=key_points[:10]
        )

    def get_summaries(self) -> List[Dict[str, Any]]:
        """Get all summaries."""
        return [
            {
                "start_time": s.start_time,
                "end_time": s.end_time,
                "message_count": s.message_count,
                "summary_text": s.summary_text,
                "key_points": s.key_points
            }
            for s in self._summaries
        ]

    def set_context(self, key: str, value: Any) -> None:
        """Set a context value."""
        self._context[key] = value

    def get_context(self, key: str, default: Any = None) -> Any:
        """Get a context value."""
        return self._context.get(key, default)

    def clear(self) -> None:
        """Clear all memory."""
        self._messages = []
        self._summaries = []
        self._context = {}

    @property
    def message_count(self) -> int:
        """Return the number of messages."""
        return len(self._messages)

    @property
    def summary_count(self) -> int:
        """Return the number of summaries."""
        return len(self._summaries)

    def to_json(self) -> str:
        """Serialize memory to JSON."""
        return json.dumps({
            "messages": [m.to_dict() for m in self._messages],
            "summaries": self.get_summaries(),
            "context": self._context
        }, ensure_ascii=False)

    @classmethod
    def from_json(cls, json_str: str) -> 'ShortTermMemory':
        """Deserialize memory from JSON."""
        data = json.loads(json_str)
        memory = cls()
        memory._messages = [
            MemoryItem.from_dict(m) for m in data.get("messages", [])
        ]
        memory._context = data.get("context", {})
        return memory
