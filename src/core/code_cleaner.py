"""
Code quality utilities.

Implements QTY-05: Remove debug print statements
"""

import ast
import re
from pathlib import Path
from typing import List, Tuple, Optional
from dataclasses import dataclass
import logging

logger = logging.getLogger(__name__)


@dataclass
class CodeIssue:
    """Represents a code quality issue."""
    file_path: str
    line_number: int
    issue_type: str
    message: str
    severity: str = "warning"


class DebugPrintChecker(ast.NodeVisitor):
    """AST visitor to find debug print statements."""

    FORBIDDEN_CALLS = {'print', 'pprint', 'pp'}

    def __init__(self, filename: str):
        self.filename = filename
        self.issues: List[CodeIssue] = []
        self._in_main = False

    def visit_If(self, node: ast.If):
        # Check for if __name__ == "__main__"
        if isinstance(node.test, ast.Compare):
            if isinstance(node.test.left, ast.Name):
                if node.test.left.id == "__name__":
                    old_in_main = self._in_main
                    self._in_main = True
                    self.generic_visit(node)
                    self._in_main = old_in_main
                    return
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call):
        if isinstance(node.func, ast.Name):
            if node.func.id in self.FORBIDDEN_CALLS:
                if not self._in_main:
                    self.issues.append(CodeIssue(
                        file_path=self.filename,
                        line_number=node.lineno,
                        issue_type="debug-print",
                        message=f"Found debug print statement: {node.func.id}()",
                        severity="warning"
                    ))
        self.generic_visit(node)


class CodeCleaner:
    """Utility for checking and fixing code quality issues."""

    DEBUG_PATTERNS = [
        r'\bprint\s*\(',
        r'\bpprint\.pprint\s*\(',
        r'\bpp\s*\(',
    ]

    def __init__(self, project_root: str = "."):
        self.project_root = Path(project_root)

    def scan_file(self, file_path: str) -> List[CodeIssue]:
        """Scan a single file for issues."""
        issues = []
        path = Path(file_path)

        if not path.exists() or path.suffix != '.py':
            return issues

        content = path.read_text(encoding='utf-8', errors='ignore')

        # Check for debug patterns
        for pattern in self.DEBUG_PATTERNS:
            for match in re.finditer(pattern, content):
                line_num = content[:match.start()].count('\n') + 1
                issues.append(CodeIssue(
                    file_path=str(path),
                    line_number=line_num,
                    issue_type="debug-pattern",
                    message=f"Found pattern: {pattern}"
                ))

        # AST-based check
        try:
            tree = ast.parse(content)
            checker = DebugPrintChecker(str(path))
            checker.visit(tree)
            issues.extend(checker.issues)
        except SyntaxError:
            pass

        return issues

    def scan_directory(self, directory: str = "src") -> List[CodeIssue]:
        """Scan all Python files in directory."""
        all_issues = []
        dir_path = self.project_root / directory

        if not dir_path.exists():
            return all_issues

        for py_file in dir_path.rglob("*.py"):
            all_issues.extend(self.scan_file(str(py_file)))

        return all_issues

    def fix_file(self, file_path: str, dry_run: bool = True) -> List[str]:
        """Fix issues in a file."""
        path = Path(file_path)
        content = path.read_text(encoding='utf-8')
        lines = content.split('\n')
        changes = []

        for i, line in enumerate(lines):
            if re.search(r'\bprint\s*\(', line) and not line.strip().startswith('#'):
                new_line = f"# {line}  # Removed debug print"
                changes.append(f"Line {i+1}: Commented print statement")
                if not dry_run:
                    lines[i] = new_line

        if not dry_run and changes:
            path.write_text('\n'.join(lines))

        return changes


def check_no_debug_prints(directory: str = "src") -> Tuple[bool, List[CodeIssue]]:
    """Check that no debug print statements exist."""
    cleaner = CodeCleaner()
    issues = cleaner.scan_directory(directory)
    debug_issues = [i for i in issues if i.issue_type == "debug-print"]
    return len(debug_issues) == 0, debug_issues


def get_logging_statement(level: str, message: str) -> str:
    """Generate proper logging statement."""
    return f'logger.{level}("{message}")'
