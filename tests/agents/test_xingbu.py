"""
Tests for AGT-04: Xingbu Agent implementation.

Xingbu (刑部) - Ministry of Justice
Responsibilities: Code review, testing, quality assurance
"""

import pytest
from typing import Dict, Any


class TestXingbuState:
    """Tests for XingbuState structure."""

    def test_xingbu_state_has_required_fields(self):
        """XingbuState must have required fields."""
        from src.agents.xingbu import XingbuState

        # Check TypedDict fields exist
        state = {
            "messages": [],
            "task_id": "test",
            "goal": "test",
            "files_reviewed": [],
            "tests_run": [],
            "issues_found": [],
            "errors": []
        }
        # Should not raise
        XingbuState(**state)

    def test_xingbu_state_messages_field(self):
        """XingbuState must have messages field with add_messages annotation."""
        from src.agents.xingbu import XingbuState
        from typing import get_type_hints
        from langgraph.graph.message import add_messages

        hints = get_type_hints(XingbuState, include_extras=True)
        # messages field should have Annotated type with add_messages
        assert "messages" in hints

    def test_xingbu_state_tracking_fields(self):
        """XingbuState must track files_reviewed, tests_run, issues_found."""
        from src.agents.xingbu import XingbuState
        from typing import get_type_hints

        hints = get_type_hints(XingbuState)
        assert "files_reviewed" in hints
        assert "tests_run" in hints
        assert "issues_found" in hints


class TestXingbuTools:
    """Tests for Xingbu Agent tools."""

    def test_xingbu_tools_defined(self):
        """Xingbu tools must be defined."""
        from src.agents.xingbu import xingbu_tools

        tool_names = [t.name for t in xingbu_tools]
        assert "tool_review_code" in tool_names
        assert "tool_run_tests" in tool_names
        assert "tool_check_coverage" in tool_names
        assert "tool_security_scan" in tool_names

    def test_tool_review_code_exists(self):
        """tool_review_code must exist."""
        from src.agents.xingbu import tool_review_code

        assert tool_review_code.name == "tool_review_code"
        assert "filepath" in tool_review_code.args_schema.schema()["properties"]
        assert "focus" in tool_review_code.args_schema.schema()["properties"]

    def test_tool_run_tests_exists(self):
        """tool_run_tests must exist."""
        from src.agents.xingbu import tool_run_tests

        assert tool_run_tests.name == "tool_run_tests"
        assert "test_path" in tool_run_tests.args_schema.schema()["properties"]
        assert "pattern" in tool_run_tests.args_schema.schema()["properties"]

    def test_tool_check_coverage_exists(self):
        """tool_check_coverage must exist."""
        from src.agents.xingbu import tool_check_coverage

        assert tool_check_coverage.name == "tool_check_coverage"
        assert "test_path" in tool_check_coverage.args_schema.schema()["properties"]
        assert "source_path" in tool_check_coverage.args_schema.schema()["properties"]

    def test_tool_security_scan_exists(self):
        """tool_security_scan must exist."""
        from src.agents.xingbu import tool_security_scan

        assert tool_security_scan.name == "tool_security_scan"
        assert "path" in tool_security_scan.args_schema.schema()["properties"]


class TestXingbuGraph:
    """Tests for Xingbu Agent graph."""

    def test_build_xingbu_graph(self):
        """build_xingbu_graph must return compiled graph."""
        from src.agents.xingbu import build_xingbu_graph

        graph = build_xingbu_graph()
        assert graph is not None

    def test_xingbu_graph_has_react_node(self):
        """Xingbu graph must have react node."""
        from src.agents.xingbu import build_xingbu_graph

        graph = build_xingbu_graph()
        # LangGraph compiled graph has nodes attribute
        assert "react" in graph.nodes

    def test_xingbu_graph_has_tools_node(self):
        """Xingbu graph must have tools node."""
        from src.agents.xingbu import build_xingbu_graph

        graph = build_xingbu_graph()
        assert "tools" in graph.nodes

    def test_xingbu_graph_follows_pattern(self):
        """Xingbu graph follows react -> tools -> react pattern."""
        from src.agents.xingbu import build_xingbu_graph

        graph = build_xingbu_graph()
        # Check graph structure: __start__ -> react, react -> tools, tools -> react
        g = graph.get_graph()
        # Find edge from __start__ to react
        start_edges = [e for e in g.edges if e.source == "__start__"]
        assert len(start_edges) == 1
        assert start_edges[0].target == "react"


class TestCodeReviewTools:
    """Tests for code_review tools."""

    def test_review_code_returns_string(self):
        """review_code should return a string result."""
        from src.tools.code_review import review_code

        result = review_code("src/tools/base.py", focus="errors")
        assert isinstance(result, str)

    def test_run_tests_returns_string(self):
        """run_tests should return a string result."""
        from unittest.mock import patch

        with patch("src.tools.code_review.subprocess.run") as mock_run:
            mock_run.return_value.stdout = "1 passed"
            mock_run.return_value.stderr = ""
            from src.tools.code_review import run_tests
            result = run_tests("tests/", pattern="test_example")
            assert isinstance(result, str)

    def test_check_coverage_returns_string(self):
        """check_coverage should return a string result."""
        from unittest.mock import patch

        with patch("src.tools.code_review.subprocess.run") as mock_run:
            mock_run.return_value.stdout = "Coverage: 80%"
            mock_run.return_value.stderr = ""
            from src.tools.code_review import check_coverage
            result = check_coverage("tests/", "src/")
            assert isinstance(result, str)

    def test_security_scan_returns_string(self):
        """run_security_scan should return a string result."""
        from unittest.mock import patch

        with patch("src.tools.code_review.subprocess.run") as mock_run:
            mock_run.return_value.stdout = "No issues found"
            mock_run.return_value.stderr = ""
            from src.tools.code_review import run_security_scan
            result = run_security_scan("src/tools/")
            assert isinstance(result, str)

    def test_review_code_handles_invalid_path(self):
        """review_code should handle invalid paths gracefully."""
        from src.tools.code_review import review_code

        result = review_code("nonexistent/path.py")
        assert "Error" in result or "not found" in result.lower()

    def test_run_tests_handles_invalid_path(self):
        """run_tests should handle invalid paths gracefully."""
        from src.tools.code_review import run_tests

        result = run_tests("nonexistent/tests/")
        assert "Error" in result or "not found" in result.lower()
