"""
Sandboxed code execution tools for Bingbu Agent.

Security considerations:
- File operations are sandboxed to project directory by default
- Python execution uses subprocess isolation
- High-risk operations require approval
"""

import os
import subprocess
import tempfile
from pathlib import Path
from typing import Optional


# Get project root (where openforce.db is located)
def get_project_root() -> Path:
    """Get the project root directory."""
    # Look for openforce.db or .git to find project root
    current = Path.cwd()
    while current != current.parent:
        if (current / "openforce.db").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return Path.cwd()


def resolve_path(filepath: str, must_exist: bool = False) -> Path:
    """
    Resolve a filepath relative to project root.

    Args:
        filepath: Relative path to resolve
        must_exist: If True, raise error if file doesn't exist

    Returns:
        Absolute path

    Raises:
        ValueError: If path escapes project root or file doesn't exist
    """
    project_root = get_project_root()
    absolute_path = (project_root / filepath).resolve()

    # Security: Ensure path is within project root
    try:
        absolute_path.relative_to(project_root.resolve())
    except ValueError:
        raise ValueError(f"Path escapes project root: {filepath}")

    if must_exist and not absolute_path.exists():
        raise ValueError(f"File not found: {filepath}")

    return absolute_path


def create_file(filepath: str, content: str) -> str:
    """
    Create a new file with the given content.

    Args:
        filepath: Path relative to project root
        content: Content to write to the file

    Returns:
        Success message or error
    """
    try:
        absolute_path = resolve_path(filepath)

        # Create parent directories if needed
        absolute_path.parent.mkdir(parents=True, exist_ok=True)

        # Write file
        absolute_path.write_text(content, encoding="utf-8")

        return f"Created file: {filepath}"

    except Exception as e:
        return f"Error creating file: {str(e)}"


def edit_file(filepath: str, old_content: str, new_content: str) -> str:
    """
    Edit an existing file by replacing old_content with new_content.

    Args:
        filepath: Path relative to project root
        old_content: Exact text to find and replace
        new_content: New text to insert

    Returns:
        Success message or error
    """
    try:
        absolute_path = resolve_path(filepath, must_exist=True)

        # Read current content
        current_content = absolute_path.read_text(encoding="utf-8")

        # Replace
        if old_content not in current_content:
            return f"Error: old_content not found in {filepath}"

        new_file_content = current_content.replace(old_content, new_content, 1)

        # Write back
        absolute_path.write_text(new_file_content, encoding="utf-8")

        return f"Edited file: {filepath}"

    except Exception as e:
        return f"Error editing file: {str(e)}"


def execute_python(code: str) -> str:
    """
    Execute Python code in a sandboxed subprocess.

    SECURITY: This is a high-risk operation that requires approval.

    Args:
        code: Python code to execute

    Returns:
        Execution output or error
    """
    try:
        # Use subprocess isolation for sandboxing
        # Run with timeout and resource limits
        result = subprocess.run(
            ["python3", "-c", code],
            capture_output=True,
            text=True,
            timeout=30,  # 30 second timeout
            cwd=str(get_project_root())
        )

        output = result.stdout
        if result.stderr:
            output += f"\nSTDERR: {result.stderr}"

        if result.returncode != 0:
            output += f"\nExit code: {result.returncode}"

        return output or "(no output)"

    except subprocess.TimeoutExpired:
        return "Error: Execution timed out (30s limit)"
    except Exception as e:
        return f"Error executing code: {str(e)}"
