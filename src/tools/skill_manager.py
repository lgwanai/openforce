"""
Skill management tools for Libu Agent.

Handles installation, updates, listing, and uninstallation of skills
via pip subprocess calls.
"""

import subprocess
import sys
from typing import Optional


def install_skill(skill_name: str, source: str = "pip") -> str:
    """
    Install a skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to install
        source: Installation source ("pip" or "git")

    Returns:
        Success or error message
    """
    try:
        if source == "git":
            cmd = [sys.executable, "-m", "pip", "install", f"git+{skill_name}"]
        else:
            cmd = [sys.executable, "-m", "pip", "install", skill_name]

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120  # 2 minute timeout for installations
        )

        if result.returncode == 0:
            return f"Successfully installed: {skill_name}"
        else:
            return f"Failed to install {skill_name}: {result.stderr}"

    except subprocess.TimeoutExpired:
        return f"Error: Installation timed out (120s limit) for {skill_name}"
    except Exception as e:
        return f"Error installing skill: {str(e)}"


def update_skill(skill_name: str) -> str:
    """
    Update an installed skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to update

    Returns:
        Success or error message
    """
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "install", "--upgrade", skill_name],
            capture_output=True,
            text=True,
            timeout=120
        )

        if result.returncode == 0:
            # Check if actually updated or already up-to-date
            if "Requirement already satisfied" in result.stdout:
                return f"{skill_name} is already up-to-date"
            return f"Successfully updated: {skill_name}"
        else:
            return f"Failed to update {skill_name}: {result.stderr}"

    except subprocess.TimeoutExpired:
        return f"Error: Update timed out (120s limit) for {skill_name}"
    except Exception as e:
        return f"Error updating skill: {str(e)}"


def list_skills() -> str:
    """
    List all installed Python packages/skills.

    Returns:
        JSON-like string listing all installed packages
    """
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "list", "--format=json"],
            capture_output=True,
            text=True,
            timeout=30
        )

        if result.returncode == 0:
            return result.stdout
        else:
            return f"Error listing skills: {result.stderr}"

    except subprocess.TimeoutExpired:
        return "Error: Listing timed out (30s limit)"
    except Exception as e:
        return f"Error listing skills: {str(e)}"


def uninstall_skill(skill_name: str) -> str:
    """
    Uninstall a skill (Python package) via pip.

    Args:
        skill_name: Name of the package/skill to uninstall

    Returns:
        Success or error message
    """
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "uninstall", "-y", skill_name],
            capture_output=True,
            text=True,
            timeout=60
        )

        if result.returncode == 0:
            return f"Successfully uninstalled: {skill_name}"
        else:
            # Check if package wasn't installed
            if "Cannot uninstall requirement" in result.stderr:
                return f"Package not installed: {skill_name}"
            return f"Failed to uninstall {skill_name}: {result.stderr}"

    except subprocess.TimeoutExpired:
        return f"Error: Uninstall timed out (60s limit) for {skill_name}"
    except Exception as e:
        return f"Error uninstalling skill: {str(e)}"
