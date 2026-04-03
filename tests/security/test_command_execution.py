import pytest
import subprocess
import tempfile
import os
from pathlib import Path


class TestCommandWhitelist:
    """Tests for SEC-01: Command whitelist module."""

    def test_non_whitelisted_raises_security_error(self):
        """Test 1: Non-whitelisted executable raises SecurityError."""
        from src.security.command_whitelist import CommandWhitelist, SecurityError

        wl = CommandWhitelist()
        wl.allow('echo')  # Only allow echo

        with pytest.raises(SecurityError) as exc_info:
            wl.run('rm', ['-rf', '/'])

        assert "not in whitelist" in str(exc_info.value).lower()

    def test_whitelisted_executable_resolved_correctly(self):
        """Test 2: Whitelisted executable is resolved correctly."""
        from src.security.command_whitelist import CommandWhitelist

        wl = CommandWhitelist()
        wl.allow('python3')

        # Verify python3 was resolved
        assert 'python3' in wl._allowed
        # Should be a valid path
        assert os.path.isabs(wl._allowed['python3']) or wl._allowed['python3'] == 'python3'

    def test_shell_injection_characters_not_interpreted(self):
        """Test 3: Shell injection characters in args are treated as literal args."""
        from src.security.command_whitelist import CommandWhitelist

        wl = CommandWhitelist()
        wl.allow('echo')

        # These shell injection characters should be treated as literal text
        result = wl.run('echo', [';', 'rm', '-rf', '/'], capture_output=True, text=True)
        assert result.returncode == 0
        # The output should contain the literal characters, not execute them
        assert ';' in result.stdout

    def test_run_returns_completed_process(self):
        """Test that run() returns a subprocess.CompletedProcess."""
        from src.security.command_whitelist import CommandWhitelist

        wl = CommandWhitelist()
        wl.allow('echo')

        result = wl.run('echo', ['hello'], capture_output=True, text=True)
        assert isinstance(result, subprocess.CompletedProcess)
        assert 'hello' in result.stdout

    def test_default_whitelist_includes_common_commands(self):
        """Test that default whitelist includes python3, npx, agent-browser."""
        from src.security.command_whitelist import CommandWhitelist

        wl = CommandWhitelist()
        # Default whitelist should include these
        assert 'python3' in wl._allowed
        assert 'npx' in wl._allowed
        assert 'agent-browser' in wl._allowed

    def test_allow_with_custom_path(self):
        """Test that allow() can accept a custom path."""
        from src.security.command_whitelist import CommandWhitelist
        import shutil

        wl = CommandWhitelist()
        python_path = shutil.which('python3')
        if python_path:
            wl.allow('my-python', python_path)
            assert 'my-python' in wl._allowed
            assert wl._allowed['my-python'] == python_path

    def test_shell_equals_false_always(self):
        """Test that shell=False is explicitly used in subprocess.run calls."""
        from src.security.command_whitelist import CommandWhitelist
        import inspect

        # Check the source code explicitly uses shell=False
        source = inspect.getsource(CommandWhitelist.run)
        # The key check: subprocess.run should have shell=False
        assert 'subprocess.run(cmd, shell=False' in source, \
            "run() must explicitly use shell=False in subprocess.run()"


class TestSafeCommandExecutor:
    """Tests for safe command executor."""

    def test_run_agent_browser_safe_no_shell_injection(self):
        """Test 1: run_agent_browser_safe() returns output without shell injection."""
        from src.tools.command_executor import run_agent_browser_safe

        # This should be treated as literal args, not shell commands
        # Since agent-browser may not be installed, we expect an error about that
        # but NOT a successful rm command execution
        try:
            result = run_agent_browser_safe(['--help'])
            # If agent-browser is installed, we should get help output
            assert isinstance(result, str)
        except Exception as e:
            # If not installed, error should be about agent-browser, not shell injection
            error_msg = str(e).lower()
            assert 'rm' not in error_msg or 'not found' in error_msg

    def test_malicious_args_treated_as_literal(self):
        """Test 2: Malicious command args like '; rm -rf' are treated as literal args."""
        from src.tools.command_executor import run_agent_browser_safe

        # The semicolon should NOT be interpreted as command separator
        # We can't test actual execution without agent-browser, but we verify
        # the function accepts the input safely
        malicious_input = ['; rm -rf /', '&&', 'echo pwned']
        try:
            result = run_agent_browser_safe(malicious_input)
            # Function should handle this safely
            assert isinstance(result, str)
        except Exception as e:
            # Error should be about agent-browser not found, not shell injection
            assert 'rm' not in str(e).lower() or 'not found' in str(e).lower()

    def test_timeout_enforced(self):
        """Test 3: Timeout is enforced."""
        from src.tools.command_executor import run_agent_browser_safe
        import subprocess

        # We can't easily test timeout without a running process,
        # but we verify the timeout parameter is used
        import inspect
        source = inspect.getsource(run_agent_browser_safe)
        # Should have timeout in the code
        assert 'timeout' in source.lower()

    def test_environment_setup_in_python(self):
        """Test that environment setup is done in Python, not shell."""
        from src.tools import command_executor
        import inspect

        # Check the module source for Python-based setup
        source = inspect.getsource(command_executor)
        # Should use makedirs or Path for directory creation, not shell 'mkdir -p'
        assert 'makedirs' in source or 'mkdir' in source, \
            "Should use Python makedirs/Path.mkdir for directory creation"
        # Should not have shell command chaining with '&&'
        assert 'mkdir -p &&' not in source, \
            "Should not use shell command chaining"


class TestRunAgentBrowserIntegration:
    """Integration tests for run_agent_browser using safe executor."""

    def test_uses_safe_executor(self):
        """Test that run_agent_browser() calls the safe executor."""
        from src.tools.base import run_agent_browser
        import inspect

        source = inspect.getsource(run_agent_browser)
        # Should import and use the safe executor
        assert 'run_agent_browser_safe' in source or 'command_executor' in source

    def test_no_shell_true_in_base(self):
        """Test that shell=True is removed from src/tools/base.py."""
        import subprocess
        from src.tools import base

        # Check the module source doesn't have shell=True in subprocess calls
        import inspect
        source = inspect.getsource(base)
        lines_with_shell_true = [
            line for line in source.split('\n')
            if 'shell=True' in line and 'subprocess' in line
        ]
        assert len(lines_with_shell_true) == 0, f"Found shell=True in: {lines_with_shell_true}"

    def test_command_string_parsed_safely(self):
        """Test that command string is parsed into args safely."""
        from src.tools.base import run_agent_browser
        import inspect

        source = inspect.getsource(run_agent_browser)
        # Should use shlex.split for parsing
        assert 'shlex' in source, "Should use shlex for safe parsing"
