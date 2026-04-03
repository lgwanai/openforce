import pytest
import tempfile
import os
from pathlib import Path


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
