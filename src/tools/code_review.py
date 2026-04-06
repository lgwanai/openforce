"""
Code review and testing tools for Xingbu Agent.

This module provides tools for:
- Code linting with ruff and mypy
- Test execution with pytest
- Coverage checking with pytest-cov
- Security scanning with bandit
"""

import subprocess
from pathlib import Path
from typing import Optional


def get_project_root() -> Path:
    """Get the project root directory."""
    current = Path.cwd()
    while current != current.parent:
        if (current / "openforce.db").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return Path.cwd()


def review_code(filepath: str, focus: str = "all") -> str:
    """
    Review code using ruff and mypy.

    Args:
        filepath: Path to the file or directory to review
        focus: Focus area - "all", "style", "types", or "errors"

    Returns:
        Review results or error message
    """
    try:
        project_root = get_project_root()
        target_path = (project_root / filepath).resolve()

        # Security: Ensure path is within project root
        try:
            target_path.relative_to(project_root.resolve())
        except ValueError:
            return f"Error: Path escapes project root: {filepath}"

        if not target_path.exists():
            return f"Error: Path not found: {filepath}"

        results = []

        # Run ruff for linting
        if focus in ("all", "style", "errors"):
            try:
                ruff_result = subprocess.run(
                    ["ruff", "check", str(target_path), "--output-format", "concise"],
                    capture_output=True,
                    text=True,
                    timeout=60,
                    cwd=str(project_root)
                )
                if ruff_result.stdout.strip():
                    results.append(f"=== Ruff Linting ===\n{ruff_result.stdout}")
                if ruff_result.returncode == 0:
                    results.append("=== Ruff Linting ===\nNo issues found.")
            except subprocess.TimeoutExpired:
                results.append("=== Ruff Linting ===\nTimeout (60s)")
            except FileNotFoundError:
                results.append("=== Ruff Linting ===\nRuff not installed. Run: pip install ruff")
            except Exception as e:
                results.append(f"=== Ruff Linting ===\nError: {str(e)}")

        # Run mypy for type checking
        if focus in ("all", "types"):
            try:
                mypy_result = subprocess.run(
                    ["mypy", str(target_path), "--no-error-summary"],
                    capture_output=True,
                    text=True,
                    timeout=120,
                    cwd=str(project_root)
                )
                if mypy_result.stdout.strip():
                    results.append(f"=== Mypy Type Checking ===\n{mypy_result.stdout}")
                if mypy_result.returncode == 0:
                    results.append("=== Mypy Type Checking ===\nNo type errors found.")
            except subprocess.TimeoutExpired:
                results.append("=== Mypy Type Checking ===\nTimeout (120s)")
            except FileNotFoundError:
                results.append("=== Mypy Type Checking ===\nMypy not installed. Run: pip install mypy")
            except Exception as e:
                results.append(f"=== Mypy Type Checking ===\nError: {str(e)}")

        return "\n\n".join(results) if results else "No review performed."

    except Exception as e:
        return f"Error reviewing code: {str(e)}"


def run_tests(test_path: str = "tests/", pattern: str = "") -> str:
    """
    Run tests using pytest.

    Args:
        test_path: Path to test file or directory
        pattern: Test name pattern to filter (e.g., "test_login")

    Returns:
        Test results or error message
    """
    try:
        project_root = get_project_root()
        target_path = (project_root / test_path).resolve()

        # Security: Ensure path is within project root
        try:
            target_path.relative_to(project_root.resolve())
        except ValueError:
            return f"Error: Path escapes project root: {test_path}"

        if not target_path.exists():
            return f"Error: Test path not found: {test_path}"

        # Build pytest command
        cmd = ["pytest", str(target_path), "-v", "--tb=short"]

        if pattern:
            cmd.extend(["-k", pattern])

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=300,  # 5 minute timeout
            cwd=str(project_root)
        )

        output = result.stdout
        if result.stderr:
            output += f"\n{result.stderr}"

        # Truncate if too long
        if len(output) > 10000:
            output = output[:10000] + "\n... [TRUNCATED]"

        return output or "(no output)"

    except subprocess.TimeoutExpired:
        return "Error: Test execution timed out (5 minute limit)"
    except FileNotFoundError:
        return "Error: pytest not installed. Run: pip install pytest"
    except Exception as e:
        return f"Error running tests: {str(e)}"


def check_coverage(test_path: str = "tests/", source_path: str = "src/") -> str:
    """
    Check test coverage using pytest-cov.

    Args:
        test_path: Path to test file or directory
        source_path: Path to source code to measure coverage for

    Returns:
        Coverage report or error message
    """
    try:
        project_root = get_project_root()
        test_target = (project_root / test_path).resolve()
        source_target = (project_root / source_path).resolve()

        # Security: Ensure paths are within project root
        try:
            test_target.relative_to(project_root.resolve())
            source_target.relative_to(project_root.resolve())
        except ValueError:
            return f"Error: Path escapes project root"

        if not test_target.exists():
            return f"Error: Test path not found: {test_path}"
        if not source_target.exists():
            return f"Error: Source path not found: {source_path}"

        result = subprocess.run(
            [
                "pytest",
                str(test_target),
                f"--cov={source_target}",
                "--cov-report=term-missing",
                "-v",
                "--tb=short"
            ],
            capture_output=True,
            text=True,
            timeout=300,  # 5 minute timeout
            cwd=str(project_root)
        )

        output = result.stdout
        if result.stderr:
            output += f"\n{result.stderr}"

        # Truncate if too long
        if len(output) > 15000:
            output = output[:15000] + "\n... [TRUNCATED]"

        return output or "(no output)"

    except subprocess.TimeoutExpired:
        return "Error: Coverage check timed out (5 minute limit)"
    except FileNotFoundError:
        return "Error: pytest-cov not installed. Run: pip install pytest-cov"
    except Exception as e:
        return f"Error checking coverage: {str(e)}"


def run_security_scan(path: str = "src/") -> str:
    """
    Run security scan using bandit.

    Args:
        path: Path to scan for security issues

    Returns:
        Security scan results or error message
    """
    try:
        project_root = get_project_root()
        target_path = (project_root / path).resolve()

        # Security: Ensure path is within project root
        try:
            target_path.relative_to(project_root.resolve())
        except ValueError:
            return f"Error: Path escapes project root: {path}"

        if not target_path.exists():
            return f"Error: Path not found: {path}"

        result = subprocess.run(
            [
                "bandit",
                "-r",
                str(target_path),
                "-f",
                "txt",
                "-ll"  # Only show medium and high severity
            ],
            capture_output=True,
            text=True,
            timeout=120,  # 2 minute timeout
            cwd=str(project_root)
        )

        output = result.stdout
        if result.stderr and "bandit" not in result.stderr.lower():
            output += f"\n{result.stderr}"

        # Truncate if too long
        if len(output) > 10000:
            output = output[:10000] + "\n... [TRUNCATED]"

        return output or "No security issues found."

    except subprocess.TimeoutExpired:
        return "Error: Security scan timed out (2 minute limit)"
    except FileNotFoundError:
        return "Error: bandit not installed. Run: pip install bandit"
    except Exception as e:
        return f"Error running security scan: {str(e)}"
