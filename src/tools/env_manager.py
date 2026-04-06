"""
Environment management tools for Gongbu Agent.

This module provides virtual environment management capabilities:
- Create virtual environments
- Run commands in environments
- Check environment status
- List and remove environments

Security: Environment operations are sandboxed to ~/.openforce/envs
"""

import os
import subprocess
import shutil
from pathlib import Path
from typing import List, Optional


def get_envs_dir() -> Path:
    """
    Get the environments directory.

    Returns:
        Path to ~/.openforce/envs directory
    """
    envs_dir = Path.home() / ".openforce" / "envs"
    envs_dir.mkdir(parents=True, exist_ok=True)
    return envs_dir


def create_env(env_name: str, python_version: str = "3.11") -> str:
    """
    Create a new virtual environment.

    Args:
        env_name: Name for the environment
        python_version: Python version to use (default: 3.11)

    Returns:
        Success message or error
    """
    try:
        envs_dir = get_envs_dir()
        env_path = envs_dir / env_name

        # Check if environment already exists
        if env_path.exists():
            return f"Error: Environment '{env_name}' already exists"

        # Find the python executable for the specified version
        python_exe = f"python{python_version}"

        # Try to create using venv module
        result = subprocess.run(
            [python_exe, "-m", "venv", str(env_path)],
            capture_output=True,
            text=True,
            timeout=120  # 2 minute timeout
        )

        if result.returncode != 0:
            # Try with python3 as fallback
            result = subprocess.run(
                ["python3", "-m", "venv", str(env_path)],
                capture_output=True,
                text=True,
                timeout=120
            )

            if result.returncode != 0:
                error_msg = result.stderr or result.stdout or "Unknown error"
                return f"Error creating environment: {error_msg}"

        # Verify environment was created
        if not env_path.exists():
            return f"Error: Environment directory not created"

        # Get python version in the new environment
        pip_path = env_path / "bin" / "pip"
        if not pip_path.exists():
            pip_path = env_path / "Scripts" / "pip.exe"  # Windows

        return f"Created environment: {env_name} at {env_path}"

    except subprocess.TimeoutExpired:
        return "Error: Environment creation timed out"
    except FileNotFoundError as e:
        return f"Error: Python executable not found: {str(e)}"
    except Exception as e:
        return f"Error creating environment: {str(e)}"


def run_command(command: str, env_name: str = "") -> str:
    """
    Run a command in a virtual environment.

    Args:
        command: Command to execute
        env_name: Environment name (empty for system Python)

    Returns:
        Command output or error
    """
    try:
        envs_dir = get_envs_dir()

        if env_name:
            env_path = envs_dir / env_name
            if not env_path.exists():
                return f"Error: Environment '{env_name}' not found"

            # Determine the bin directory
            if os.name == "nt":  # Windows
                bin_dir = env_path / "Scripts"
            else:  # Unix-like
                bin_dir = env_path / "bin"

            # Set up environment variables
            env = os.environ.copy()
            env["PATH"] = str(bin_dir) + os.pathsep + env.get("PATH", "")
            env["VIRTUAL_ENV"] = str(env_path)

            # Remove PYTHONHOME to avoid conflicts
            env.pop("PYTHONHOME", None)

            shell = True
            cwd = str(env_path)
        else:
            env = os.environ.copy()
            shell = True
            cwd = str(envs_dir)

        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=300,  # 5 minute timeout
            shell=shell,
            env=env,
            cwd=cwd
        )

        output = result.stdout
        if result.stderr:
            output += f"\nSTDERR: {result.stderr}"

        if result.returncode != 0:
            output += f"\nExit code: {result.returncode}"

        return output or "(no output)"

    except subprocess.TimeoutExpired:
        return "Error: Command execution timed out (5 minute limit)"
    except Exception as e:
        return f"Error running command: {str(e)}"


def check_env(env_name: str) -> str:
    """
    Check the status of a virtual environment.

    Args:
        env_name: Name of the environment to check

    Returns:
        Status information or error
    """
    try:
        envs_dir = get_envs_dir()
        env_path = envs_dir / env_name

        if not env_path.exists():
            return f"Error: Environment '{env_name}' not found"

        # Check for key files
        if os.name == "nt":  # Windows
            python_path = env_path / "Scripts" / "python.exe"
            pip_path = env_path / "Scripts" / "pip.exe"
        else:  # Unix-like
            python_path = env_path / "bin" / "python"
            pip_path = env_path / "bin" / "pip"

        status_parts = [f"Environment: {env_name}"]
        status_parts.append(f"Path: {env_path}")

        if python_path.exists():
            # Get Python version
            result = subprocess.run(
                [str(python_path), "--version"],
                capture_output=True,
                text=True,
                timeout=10
            )
            if result.returncode == 0:
                status_parts.append(f"Python: {result.stdout.strip()}")

            # Get installed packages count
            if pip_path.exists():
                result = subprocess.run(
                    [str(pip_path), "list", "--format=freeze"],
                    capture_output=True,
                    text=True,
                    timeout=30
                )
                if result.returncode == 0:
                    packages = [line for line in result.stdout.strip().split("\n") if line]
                    status_parts.append(f"Packages: {len(packages)} installed")
        else:
            status_parts.append("Status: Incomplete (missing Python executable)")

        return "\n".join(status_parts)

    except subprocess.TimeoutExpired:
        return "Error: Check operation timed out"
    except Exception as e:
        return f"Error checking environment: {str(e)}"


def list_envs() -> str:
    """
    List all virtual environments.

    Returns:
        JSON string of environment names
    """
    import json

    try:
        envs_dir = get_envs_dir()
        envs = []

        for item in envs_dir.iterdir():
            if item.is_dir():
                # Check if it looks like a venv
                if os.name == "nt":
                    is_venv = (item / "Scripts" / "python.exe").exists()
                else:
                    is_venv = (item / "bin" / "python").exists()

                envs.append({
                    "name": item.name,
                    "path": str(item),
                    "valid": is_venv
                })

        return json.dumps(envs, ensure_ascii=False, indent=2)

    except Exception as e:
        return json.dumps({"error": str(e)})


def remove_env(env_name: str) -> str:
    """
    Remove a virtual environment.

    WARNING: This is a destructive operation.

    Args:
        env_name: Name of the environment to remove

    Returns:
        Success message or error
    """
    try:
        envs_dir = get_envs_dir()
        env_path = envs_dir / env_name

        if not env_path.exists():
            return f"Error: Environment '{env_name}' not found"

        # Safety check: ensure it's within envs_dir
        try:
            env_path.resolve().relative_to(envs_dir.resolve())
        except ValueError:
            return f"Error: Invalid environment path"

        # Remove the environment
        shutil.rmtree(env_path)

        return f"Removed environment: {env_name}"

    except Exception as e:
        return f"Error removing environment: {str(e)}"
