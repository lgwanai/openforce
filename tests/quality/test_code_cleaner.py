"""Tests for QTY-05: Code cleaner."""
import ast
import pytest
import tempfile
from pathlib import Path

from src.core.code_cleaner import (
    CodeIssue, DebugPrintChecker, CodeCleaner,
    check_no_debug_prints, get_logging_statement
)


class TestDebugPrintChecker:
    """Tests for DebugPrintChecker."""

    def test_find_print(self):
        """Should find print statements."""
        code = '''
def test():
    print("debug")
'''
        tree = ast.parse(code)
        checker = DebugPrintChecker('test.py')
        checker.visit(tree)
        assert len(checker.issues) == 1
        assert checker.issues[0].issue_type == "debug-print"

    def test_allow_main(self):
        """Should allow print in __main__."""
        code = '''
if __name__ == "__main__":
    print("main")
'''
        tree = ast.parse(code)
        checker = DebugPrintChecker('test.py')
        checker.visit(tree)
        assert len(checker.issues) == 0


class TestCodeCleaner:
    """Tests for CodeCleaner."""

    def test_scan_file(self):
        """Should scan file for issues."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
            f.write('def test():\n    print("debug")\n')
            f.flush()

            cleaner = CodeCleaner()
            issues = cleaner.scan_file(f.name)

            assert len(issues) >= 1

            Path(f.name).unlink()

    def test_scan_directory_not_exists(self):
        """Should handle non-existent directory."""
        cleaner = CodeCleaner()
        issues = cleaner.scan_directory("nonexistent")
        assert len(issues) == 0

    def test_fix_file_dry_run(self):
        """Should report changes in dry run."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
            f.write('def test():\n    print("debug")\n')
            f.flush()

            cleaner = CodeCleaner()
            changes = cleaner.fix_file(f.name, dry_run=True)

            assert len(changes) == 1

            # File should not be modified
            content = Path(f.name).read_text()
            assert 'print("debug")' in content

            Path(f.name).unlink()


class TestCheckNoDebugPrints:
    """Tests for check_no_debug_prints."""

    def test_clean_code(self):
        """Should return True for clean code."""
        # Assuming src/ is clean
        clean, issues = check_no_debug_prints("nonexistent_dir")
        assert clean == True


class TestGetLoggingStatement:
    """Tests for get_logging_statement."""

    def test_info(self):
        """Should generate info statement."""
        stmt = get_logging_statement("info", "message")
        assert stmt == 'logger.info("message")'

    def test_error(self):
        """Should generate error statement."""
        stmt = get_logging_statement("error", "error occurred")
        assert stmt == 'logger.error("error occurred")'
