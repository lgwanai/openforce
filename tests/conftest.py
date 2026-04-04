import pytest
import tempfile
import os
from pathlib import Path
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
    """Create an isolated in-memory SQLite database for each test.

    Uses SQLite shared-cache mode to ensure all connections to the same
    in-memory database share the same data.
    """
    import sqlite3
    from src.core import db as db_module

    # Use shared-cache in-memory database
    # "file::memory:?cache=shared" allows multiple connections to share data
    shared_db_path = "file::memory:?cache=shared"

    # Create the shared connection and initialize tables
    conn = sqlite3.connect(shared_db_path, uri=True, check_same_thread=False)

    # Create tables
    conn.execute('''
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

    conn.execute('''
        CREATE TABLE IF NOT EXISTS active_tasks (
            owner_user_id TEXT PRIMARY KEY,
            task_id TEXT
        )
    ''')

    conn.execute('''
        CREATE TABLE IF NOT EXISTS used_nonces (
            nonce TEXT PRIMARY KEY,
            consumed_at TEXT
        )
    ''')
    conn.commit()

    # Monkeypatch the DB_PATH to use shared in-memory database
    monkeypatch.setattr(db_module, 'DB_PATH', shared_db_path)

    yield conn

    conn.close()


@pytest.fixture
def temp_sandbox():
    """Create a temporary sandbox directory for file operations."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


@pytest.fixture
def mock_settings(monkeypatch):
    """Mock external tools configuration."""
    monkeypatch.setenv('OPENFORCE_BAIDU_SEARCH_SCRIPT', '/tmp/test_search.py')
    monkeypatch.setenv('OPENFORCE_SECURITY_APPROVAL_SECRET_KEY', 'test-secret-key-12345')
    yield


@pytest.fixture
def token_manager():
    """Create a token manager for testing.

    Note: This fixture requires src.security.approval module which will be
    created in SEC-02. Tests using this fixture will be skipped if the
    module is not available.
    """
    try:
        from src.security.approval import ApprovalTokenManager
        return ApprovalTokenManager(secret_key=b'test-secret-key-for-testing-only')
    except ImportError:
        pytest.skip("ApprovalTokenManager not yet implemented (SEC-02)")


@pytest.fixture
def command_whitelist():
    """Create a command whitelist for testing.

    Note: This fixture requires src.security.command_whitelist module which
    will be created in SEC-01. Tests using this fixture will be skipped if
    the module is not available.
    """
    try:
        from src.security.command_whitelist import CommandWhitelist
        wl = CommandWhitelist()
        wl.allow('python3')
        wl.allow('echo')
        return wl
    except ImportError:
        pytest.skip("CommandWhitelist not yet implemented (SEC-01)")
