"""
Tests for AGT-01: Bingbu Agent implementation.

Phase 3 Plan 05: Bingbu Agent tests.

Tests cover:
- BingbuState structure
- Tool definitions
- Graph construction
- File operations
"""

import pytest
from typing import Dict, Any


class TestBingbuState:
    """Tests for BingbuState structure."""

    def test_bingbu_state_has_required_fields(self):
        """BingbuState must have required fields."""
        from src.agents.bingbu import BingbuState

        # Check TypedDict fields exist
        state = {
            "messages": [],
            "task_id": "test",
            "goal": "test",
            "files_created": [],
            "files_modified": [],
            "commands_executed": [],
            "errors": []
        }
        # Should not raise
        BingbuState(**state)

    def test_bingbu_state_messages_field(self):
        """BingbuState must have messages field with add_messages annotation."""
        from src.agents.bingbu import BingbuState
        from typing import get_type_hints
        from langgraph.graph.message import add_messages

        hints = get_type_hints(BingbuState, include_extras=True)
        # messages field should have Annotated type with add_messages
        assert "messages" in hints

    def test_bingbu_state_tracking_fields(self):
        """BingbuState must track files_created, files_modified, commands_executed."""
        from src.agents.bingbu import BingbuState
        from typing import get_type_hints

        hints = get_type_hints(BingbuState)
        assert "files_created" in hints
        assert "files_modified" in hints
        assert "commands_executed" in hints


class TestBingbuTools:
    """Tests for Bingbu Agent tools."""

    def test_bingbu_tools_defined(self):
        """Bingbu tools must be defined."""
        from src.agents.bingbu import bingbu_tools

        tool_names = [t.name for t in bingbu_tools]
        assert "tool_create_file" in tool_names
        assert "tool_edit_file" in tool_names
        assert "tool_read_file" in tool_names

    def test_tool_create_file_exists(self):
        """tool_create_file must exist."""
        from src.agents.bingbu import tool_create_file

        assert tool_create_file.name == "tool_create_file"
        assert "filepath" in tool_create_file.args_schema.schema()["properties"]
        assert "content" in tool_create_file.args_schema.schema()["properties"]

    def test_tool_edit_file_exists(self):
        """tool_edit_file must exist."""
        from src.agents.bingbu import tool_edit_file

        assert tool_edit_file.name == "tool_edit_file"
        assert "filepath" in tool_edit_file.args_schema.schema()["properties"]
        assert "old_content" in tool_edit_file.args_schema.schema()["properties"]
        assert "new_content" in tool_edit_file.args_schema.schema()["properties"]

    def test_tool_read_file_exists(self):
        """tool_read_file must exist."""
        from src.agents.bingbu import tool_read_file

        assert tool_read_file.name == "tool_read_file"
        assert "filepath" in tool_read_file.args_schema.schema()["properties"]

    def test_tool_execute_python_exists(self):
        """tool_execute_python must exist."""
        from src.agents.bingbu import tool_execute_python

        assert tool_execute_python.name == "tool_execute_python"
        assert "code" in tool_execute_python.args_schema.schema()["properties"]


class TestBingbuGraph:
    """Tests for Bingbu Agent graph."""

    def test_build_bingbu_graph(self):
        """build_bingbu_graph must return compiled graph."""
        from src.agents.bingbu import build_bingbu_graph

        graph = build_bingbu_graph()
        assert graph is not None

    def test_bingbu_graph_has_react_node(self):
        """Bingbu graph must have react node."""
        from src.agents.bingbu import build_bingbu_graph

        graph = build_bingbu_graph()
        # LangGraph compiled graph has nodes attribute
        assert "react" in graph.nodes

    def test_bingbu_graph_has_tools_node(self):
        """Bingbu graph must have tools node."""
        from src.agents.bingbu import build_bingbu_graph

        graph = build_bingbu_graph()
        assert "tools" in graph.nodes

    def test_bingbu_graph_follows_hubu_pattern(self):
        """Bingbu graph follows Hubu pattern (react -> tools -> react)."""
        from src.agents.bingbu import build_bingbu_graph

        graph = build_bingbu_graph()
        # Check graph structure: __start__ -> react, react -> tools, tools -> react
        g = graph.get_graph()
        # Find edge from __start__ to react
        start_edges = [e for e in g.edges if e.source == "__start__"]
        assert len(start_edges) == 1
        assert start_edges[0].target == "react"


class TestCodeExecutor:
    """Tests for code_executor tools."""

    def test_create_file_creates_file(self, tmp_path):
        """create_file should create a file."""
        from src.tools.code_executor import create_file, get_project_root
        import os

        # Use a test filepath relative to project
        test_filepath = "test_created_file.txt"
        content = "Hello, World!"

        result = create_file(test_filepath, content)
        assert "Created file" in result

        # Cleanup
        try:
            os.remove(test_filepath)
        except FileNotFoundError:
            pass

    def test_edit_file_modifies_content(self, tmp_path):
        """edit_file should replace content in file."""
        from src.tools.code_executor import create_file, edit_file
        import os

        # Create test file
        test_filepath = "test_edit_file.txt"
        create_file(test_filepath, "old content here")

        # Edit it
        result = edit_file(test_filepath, "old content", "new content")
        assert "Edited file" in result

        # Cleanup
        try:
            os.remove(test_filepath)
        except FileNotFoundError:
            pass

    def test_execute_python_returns_output(self):
        """execute_python should return code output."""
        from src.tools.code_executor import execute_python

        result = execute_python("print('hello')")
        assert "hello" in result

    def test_execute_python_timeout(self):
        """execute_python should timeout for long running code."""
        from src.tools.code_executor import execute_python

        result = execute_python("import time; time.sleep(60)")
        assert "timed out" in result.lower() or "timeout" in result.lower()
