"""
Security audit tools for Duchayuan Agent.

This module provides security scanning capabilities:
- Code vulnerability scanning (bandit)
- Hardcoded secrets detection
- Dependency vulnerability checking (pip-audit)
- Security report generation
"""

import os
import re
import json
import subprocess
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from enum import Enum


class Severity(Enum):
    """Vulnerability severity levels."""
    ALL = "all"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


# Secret patterns for detecting hardcoded credentials
SECRET_PATTERNS: Dict[str, Tuple[str, re.Pattern]] = {
    "api_key": (
        "API Key",
        re.compile(
            r"(?i)(api[_-]?key|apikey)\s*[=:]\s*['\"]?[a-zA-Z0-9_\-]{20,}['\"]?",
            re.IGNORECASE
        )
    ),
    "secret_key": (
        "Secret Key",
        re.compile(
            r"(?i)(secret[_-]?key|secretkey)\s*[=:]\s*['\"]?[a-zA-Z0-9_\-]{20,}['\"]?",
            re.IGNORECASE
        )
    ),
    "password": (
        "Password",
        re.compile(
            r"(?i)(password|passwd|pwd)\s*[=:]\s*['\"]?[^\s'\"]{8,}['\"]?",
            re.IGNORECASE
        )
    ),
    "token": (
        "Token",
        re.compile(
            r"(?i)(token|access_token|auth_token|bearer)\s*[=:]\s*['\"]?[a-zA-Z0-9_\-\.]{20,}['\"]?",
            re.IGNORECASE
        )
    ),
    "private_key": (
        "Private Key",
        re.compile(
            r"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----",
            re.IGNORECASE
        )
    ),
    "aws_key": (
        "AWS Access Key",
        re.compile(
            r"(?:AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}",
            re.IGNORECASE
        )
    ),
    "github_token": (
        "GitHub Token",
        re.compile(
            r"(?:ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9]{36}",
            re.IGNORECASE
        )
    ),
}


def get_project_root() -> Path:
    """Get the project root directory."""
    current = Path.cwd()
    while current != current.parent:
        if (current / "openforce.db").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return Path.cwd()


def resolve_path(path: str) -> Path:
    """
    Resolve a path relative to project root.

    Args:
        path: Relative or absolute path

    Returns:
        Resolved absolute path
    """
    project_root = get_project_root()
    if os.path.isabs(path):
        return Path(path)
    return (project_root / path).resolve()


def security_scan(path: str = "src/", severity: str = "all") -> str:
    """
    Run bandit security scan on the specified path.

    Args:
        path: Directory or file path to scan
        severity: Minimum severity level to report (all, low, medium, high, critical)

    Returns:
        JSON string with scan results
    """
    try:
        target_path = resolve_path(path)

        if not target_path.exists():
            return json.dumps({
                "error": f"Path not found: {path}",
                "vulnerabilities": []
            })

        # Build bandit command
        cmd = ["bandit", "-r", "-f", "json"]

        # Add severity filter
        if severity != "all":
            severity_map = {
                "low": "-l",
                "medium": "-m",
                "high": "-h",
                "critical": "-h"  # bandit doesn't have critical, use high
            }
            if severity.lower() in severity_map:
                cmd.append(severity_map[severity.lower()])

        cmd.append(str(target_path))

        # Run bandit
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120
        )

        # Parse results
        try:
            if result.stdout:
                scan_data = json.loads(result.stdout)
                vulnerabilities = []

                for issue in scan_data.get("results", []):
                    vulnerabilities.append({
                        "id": issue.get("test_id", ""),
                        "name": issue.get("test_name", ""),
                        "severity": issue.get("issue_severity", "UNKNOWN"),
                        "confidence": issue.get("issue_confidence", "UNKNOWN"),
                        "file": issue.get("filename", ""),
                        "line": issue.get("line_number", 0),
                        "message": issue.get("issue_text", ""),
                        "cwe": issue.get("issue_cwe", {}).get("id", "")
                    })

                return json.dumps({
                    "tool": "bandit",
                    "path": str(target_path),
                    "severity_filter": severity,
                    "total_issues": len(vulnerabilities),
                    "vulnerabilities": vulnerabilities
                }, indent=2)
        except json.JSONDecodeError:
            pass

        # Fallback: return raw output
        return json.dumps({
            "tool": "bandit",
            "path": str(target_path),
            "raw_output": result.stdout or result.stderr,
            "return_code": result.returncode,
            "vulnerabilities": []
        })

    except subprocess.TimeoutExpired:
        return json.dumps({
            "error": "Security scan timed out (120s limit)",
            "vulnerabilities": []
        })
    except FileNotFoundError:
        return json.dumps({
            "error": "bandit not installed. Install with: pip install bandit",
            "vulnerabilities": []
        })
    except Exception as e:
        return json.dumps({
            "error": f"Security scan failed: {str(e)}",
            "vulnerabilities": []
        })


def check_secrets(path: str = "src/") -> str:
    """
    Scan files for hardcoded secrets and credentials.

    Args:
        path: Directory or file path to scan

    Returns:
        JSON string with detected secrets
    """
    try:
        target_path = resolve_path(path)

        if not target_path.exists():
            return json.dumps({
                "error": f"Path not found: {path}",
                "secrets_found": []
            })

        findings = []
        files_scanned = 0

        # File extensions to scan
        scan_extensions = {".py", ".js", ".ts", ".jsx", ".tsx", ".env", ".yaml", ".yml", ".json", ".conf", ".cfg", ".ini"}

        def scan_file(file_path: Path):
            """Scan a single file for secrets."""
            nonlocal files_scanned

            # Skip binary files and certain directories
            if file_path.suffix not in scan_extensions and file_path.name not in {".env", "Dockerfile"}:
                return

            # Skip common non-source directories
            if any(part in file_path.parts for part in {"node_modules", ".git", "__pycache__", "venv", ".venv", "build", "dist"}):
                return

            try:
                content = file_path.read_text(encoding="utf-8", errors="ignore")
                files_scanned += 1

                for pattern_name, (pattern_desc, pattern_regex) in SECRET_PATTERNS.items():
                    matches = pattern_regex.finditer(content)
                    for match in matches:
                        # Get line number
                        line_num = content[:match.start()].count("\n") + 1

                        # Mask the actual secret
                        matched_text = match.group()
                        masked = matched_text[:10] + "..." if len(matched_text) > 10 else matched_text

                        findings.append({
                            "type": pattern_name,
                            "description": pattern_desc,
                            "file": str(file_path.relative_to(get_project_root())),
                            "line": line_num,
                            "masked_match": masked,
                            "severity": "HIGH" if pattern_name in {"private_key", "aws_key", "github_token"} else "MEDIUM"
                        })
            except Exception:
                pass  # Skip files that can't be read

        # Scan directory or single file
        if target_path.is_file():
            scan_file(target_path)
        else:
            for file_path in target_path.rglob("*"):
                if file_path.is_file():
                    scan_file(file_path)

        return json.dumps({
            "tool": "secret_scanner",
            "path": str(target_path),
            "files_scanned": files_scanned,
            "total_findings": len(findings),
            "secrets_found": findings
        }, indent=2)

    except Exception as e:
        return json.dumps({
            "error": f"Secret scan failed: {str(e)}",
            "secrets_found": []
        })


def check_dependencies(path: str = ".") -> str:
    """
    Check dependencies for known vulnerabilities using pip-audit.

    Args:
        path: Project root path containing requirements.txt or pyproject.toml

    Returns:
        JSON string with vulnerability findings
    """
    try:
        target_path = resolve_path(path)

        if not target_path.exists():
            return json.dumps({
                "error": f"Path not found: {path}",
                "vulnerabilities": []
            })

        # Check for pip-audit
        try:
            subprocess.run(
                ["pip-audit", "--version"],
                capture_output=True,
                check=True
            )
        except (subprocess.CalledProcessError, FileNotFoundError):
            return json.dumps({
                "error": "pip-audit not installed. Install with: pip install pip-audit",
                "vulnerabilities": []
            })

        # Find requirements file
        requirements_files = []
        for req_file in ["requirements.txt", "requirements-dev.txt", "pyproject.toml"]:
            req_path = target_path / req_file
            if req_path.exists():
                requirements_files.append(req_path)

        if not requirements_files:
            return json.dumps({
                "error": "No requirements.txt or pyproject.toml found",
                "vulnerabilities": []
            })

        all_vulnerabilities = []

        for req_file in requirements_files:
            cmd = ["pip-audit", "-r", str(req_file), "-f", "json"]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=120
            )

            # pip-audit returns non-zero when vulnerabilities found
            try:
                if result.stdout:
                    audit_data = json.loads(result.stdout)
                    for vuln in audit_data.get("vulnerabilities", []):
                        all_vulnerabilities.append({
                            "package": vuln.get("name", ""),
                            "version": vuln.get("version", ""),
                            "id": vuln.get("id", ""),
                            "severity": "HIGH",  # pip-audit doesn't provide severity
                            "description": vuln.get("description", ""),
                            "fix_version": vuln.get("fix_versions", [""])[0] if vuln.get("fix_versions") else "",
                            "source": str(req_file.relative_to(get_project_root()))
                        })
            except json.JSONDecodeError:
                pass

        return json.dumps({
            "tool": "pip-audit",
            "path": str(target_path),
            "requirements_files": [str(f.relative_to(get_project_root())) for f in requirements_files],
            "total_vulnerabilities": len(all_vulnerabilities),
            "vulnerabilities": all_vulnerabilities
        }, indent=2)

    except subprocess.TimeoutExpired:
        return json.dumps({
            "error": "Dependency check timed out (120s limit)",
            "vulnerabilities": []
        })
    except Exception as e:
        return json.dumps({
            "error": f"Dependency check failed: {str(e)}",
            "vulnerabilities": []
        })


def generate_security_report(path: str = "src/") -> str:
    """
    Generate a comprehensive security report combining all checks.

    Args:
        path: Directory to scan

    Returns:
        JSON string with complete security report
    """
    report = {
        "tool": "security_report",
        "path": path,
        "timestamp": __import__("datetime").datetime.now().isoformat(),
        "summary": {
            "code_issues": 0,
            "secrets_found": 0,
            "dependency_issues": 0,
            "total_issues": 0
        },
        "code_scan": {},
        "secrets_scan": {},
        "dependency_scan": {}
    }

    # Run code security scan
    try:
        code_result = security_scan(path, "all")
        code_data = json.loads(code_result)
        report["code_scan"] = code_data
        if "total_issues" in code_data:
            report["summary"]["code_issues"] = code_data["total_issues"]
    except Exception as e:
        report["code_scan"] = {"error": str(e)}

    # Run secrets scan
    try:
        secrets_result = check_secrets(path)
        secrets_data = json.loads(secrets_result)
        report["secrets_scan"] = secrets_data
        if "total_findings" in secrets_data:
            report["summary"]["secrets_found"] = secrets_data["total_findings"]
    except Exception as e:
        report["secrets_scan"] = {"error": str(e)}

    # Run dependency scan
    try:
        dep_result = check_dependencies(".")
        dep_data = json.loads(dep_result)
        report["dependency_scan"] = dep_data
        if "total_vulnerabilities" in dep_data:
            report["summary"]["dependency_issues"] = dep_data["total_vulnerabilities"]
    except Exception as e:
        report["dependency_scan"] = {"error": str(e)}

    # Calculate total
    report["summary"]["total_issues"] = (
        report["summary"]["code_issues"] +
        report["summary"]["secrets_found"] +
        report["summary"]["dependency_issues"]
    )

    return json.dumps(report, indent=2)
