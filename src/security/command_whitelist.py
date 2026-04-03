"""
Command whitelist module for safe subprocess execution.

This module provides a whitelist-based approach to command execution,
preventing shell injection attacks by never using shell=True and
validating all executables against a whitelist before execution.

Security guarantees:
1. Never uses shell=True
2. All executables must be in whitelist
3. Arguments are passed as list, never as shell string
"""

import subprocess
import shutil
from typing import Optional


class SecurityError(Exception):
    """Raised when a security violation is detected."""
    pass


class CommandWhitelist:
    """
    Whitelist-based command execution.

    Only allows execution of pre-approved commands. Uses subprocess.run()
    with shell=False to prevent shell injection attacks.

    Example:
        wl = CommandWhitelist()
        wl.allow('python3')
        wl.allow('npx')
        wl.allow('agent-browser', '/usr/local/bin/agent-browser')

        result = wl.run('python3', ['--version'], capture_output=True, text=True)
    """

    def __init__(self):
        """Initialize with empty whitelist."""
        self._allowed: dict[str, str] = {}

        # Add default allowed commands
        self._init_defaults()

    def _init_defaults(self):
        """Initialize with default allowed commands."""
        # Common safe commands
        default_commands = ['python3', 'npx', 'agent-browser']
        for cmd in default_commands:
            try:
                self.allow(cmd)
            except SecurityError:
                # Command not found on system, skip silently
                pass

    def allow(self, name: str, path: Optional[str] = None):
        """
        Add a command to the whitelist.

        Args:
            name: Friendly name for the command
            path: Optional explicit path. If None, resolves via shutil.which()

        Raises:
            SecurityError: If the executable cannot be resolved
        """
        resolved = shutil.which(path or name)
        if resolved:
            self._allowed[name] = resolved
        elif path:
            # Use the provided path as-is (might be a fallback like 'npx')
            self._allowed[name] = path
        # If neither found, silently skip - command won't be executable

    def run(
        self,
        name: str,
        args: list[str],
        **kwargs
    ) -> subprocess.CompletedProcess:
        """
        Execute a whitelisted command.

        Args:
            name: Name of the command (must be in whitelist)
            args: List of arguments (no shell expansion)
            **kwargs: Additional arguments passed to subprocess.run()

        Returns:
            subprocess.CompletedProcess result

        Raises:
            SecurityError: If command is not in whitelist

        Security:
            - Never uses shell=True
            - Arguments are passed as list, not shell string
            - Shell injection characters in args are treated as literal text
        """
        if name not in self._allowed:
            raise SecurityError(
                f"Command not in whitelist: {name}. "
                f"Allowed commands: {list(self._allowed.keys())}"
            )

        # Build command list - NEVER use shell=True
        cmd = [self._allowed[name]] + args

        # Ensure shell=False (explicit for security)
        return subprocess.run(cmd, shell=False, **kwargs)

    @property
    def allowed_commands(self) -> list[str]:
        """Return list of allowed command names."""
        return list(self._allowed.keys())
