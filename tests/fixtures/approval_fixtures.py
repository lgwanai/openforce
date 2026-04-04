"""
Test fixtures for Human-in-the-loop approval workflow.

Provides fixtures for:
- ApprovalRequest creation
- Mock task records
- Token management
- In-memory database isolation
"""

import pytest
import tempfile
import os
from typing import Dict, Any, Optional
from dataclasses import dataclass


@dataclass
class MockApprovalRequest:
    """Mock approval request for testing."""
    tool_name: str
    tool_args: Dict[str, Any]
    tool_call_id: str
    action_hash: str
    approval_id: str
    task_id: str
    owner_user_id: str
    snapshot: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "tool_name": self.tool_name,
            "tool_args": self.tool_args,
            "tool_call_id": self.tool_call_id,
            "action_hash": self.action_hash,
            "approval_id": self.approval_id,
            "task_id": self.task_id,
            "owner_user_id": self.owner_user_id
        }


@pytest.fixture
def approval_request():
    """Create a mock approval request for testing."""
    return MockApprovalRequest(
        tool_name="execute_command",
        tool_args={"command": "ls -la"},
        tool_call_id="call_test_001",
        action_hash="abc123def456",
        approval_id="apr_test_001",
        task_id="task_test_001",
        owner_user_id="user_test_001"
    )


@pytest.fixture
def mock_task_record():
    """Create a mock TaskRecord for testing."""
    try:
        from src.core.db import TaskRecord
        return TaskRecord(
            task_id="task_test_001",
            owner_user_id="user_test_001",
            conversation_id="conv_test_001",
            thread_id="thread_test_001",
            original_req="Test request for approval workflow",
            status="Running",
            checkpoints=[]
        )
    except ImportError:
        pytest.skip("TaskRecord not available")


@pytest.fixture
def temp_db(monkeypatch):
    """Create an isolated in-memory SQLite database for each test."""
    import sqlite3
    from src.core import db as db_module

    # Create in-memory connection
    conn = sqlite3.connect(":memory:", isolation_level="EXCLUSIVE")
    c = conn.cursor()

    # Create tables
    c.execute('''
        CREATE TABLE IF NOT EXISTS tasks (
            task_id TEXT PRIMARY KEY,
            owner_user_id TEXT,
            conversation_id TEXT,
            thread_id TEXT,
            data JSON,
            status TEXT,
            created_at TEXT,
            updated_at TEXT
        )
    ''')

    c.execute('''
        CREATE TABLE IF NOT EXISTS active_tasks (
            owner_user_id TEXT PRIMARY KEY,
            task_id TEXT
        )
    ''')

    c.execute('''
        CREATE TABLE IF NOT EXISTS used_nonces (
            nonce TEXT PRIMARY KEY,
            consumed_at TEXT
        )
    ''')
    conn.commit()

    # Monkeypatch the DB_PATH to use in-memory database
    original_connect = sqlite3.connect
    original_db_path = db_module.DB_PATH

    def mock_connect(path, *args, **kwargs):
        if path == original_db_path:
            return conn
        return original_connect(path, *args, **kwargs)

    monkeypatch.setattr(sqlite3, 'connect', mock_connect)

    yield conn

    conn.close()
