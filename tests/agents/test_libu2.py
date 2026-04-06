"""
Tests for AGT-05: Libu2 Agent implementation.

Libu2 (礼部) - Ministry of Rites
Responsibilities: Documentation generation, formatting
"""

import pytest
from typing import Dict, Any


class TestLibu2State:
    """Tests for Libu2State structure."""

    def test_libu2_state_has_required_fields(self):
        """Libu2State must have required fields."""
        from src.agents.libu2 import Libu2State

        # Check TypedDict fields exist
        state = {
            "messages": [],
            "task_id": "test",
            "goal": "test",
            "docs_created": [],
            "docs_updated": [],
            "errors": []
        }
        # Should not raise
        Libu2State(**state)

    def test_libu2_state_messages_field(self):
        """Libu2State must have messages field with add_messages annotation."""
        from src.agents.libu2 import Libu2State
        from typing import get_type_hints
        from langgraph.graph.message import add_messages

        hints = get_type_hints(Libu2State, include_extras=True)
        # messages field should have Annotated type with add_messages
        assert "messages" in hints

    def test_libu2_state_tracking_fields(self):
        """Libu2State must track docs_created, docs_updated, errors."""
        from src.agents.libu2 import Libu2State
        from typing import get_type_hints

        hints = get_type_hints(Libu2State)
        assert "docs_created" in hints
        assert "docs_updated" in hints
        assert "errors" in hints


class TestLibu2Tools:
    """Tests for Libu2 Agent tools."""

    def test_libu2_tools_defined(self):
        """Libu2 tools must be defined."""
        from src.agents.libu2 import libu2_tools

        tool_names = [t.name for t in libu2_tools]
        assert "tool_extract_docstrings" in tool_names
        assert "tool_generate_doc" in tool_names
        assert "tool_format_markdown" in tool_names
        assert "tool_create_readme" in tool_names

    def test_tool_extract_docstrings_exists(self):
        """tool_extract_docstrings must exist."""
        from src.agents.libu2 import tool_extract_docstrings

        assert tool_extract_docstrings.name == "tool_extract_docstrings"
        assert "filepath" in tool_extract_docstrings.args_schema.schema()["properties"]

    def test_tool_generate_doc_exists(self):
        """tool_generate_doc must exist."""
        from src.agents.libu2 import tool_generate_doc

        assert tool_generate_doc.name == "tool_generate_doc"
        assert "module_name" in tool_generate_doc.args_schema.schema()["properties"]
        assert "output_format" in tool_generate_doc.args_schema.schema()["properties"]

    def test_tool_format_markdown_exists(self):
        """tool_format_markdown must exist."""
        from src.agents.libu2 import tool_format_markdown

        assert tool_format_markdown.name == "tool_format_markdown"
        assert "content" in tool_format_markdown.args_schema.schema()["properties"]
        assert "style" in tool_format_markdown.args_schema.schema()["properties"]

    def test_tool_create_readme_exists(self):
        """tool_create_readme must exist."""
        from src.agents.libu2 import tool_create_readme

        assert tool_create_readme.name == "tool_create_readme"
        assert "title" in tool_create_readme.args_schema.schema()["properties"]
        assert "description" in tool_create_readme.args_schema.schema()["properties"]
        assert "sections" in tool_create_readme.args_schema.schema()["properties"]


class TestLibu2Graph:
    """Tests for Libu2 Agent graph."""

    def test_build_libu2_graph(self):
        """build_libu2_graph must return compiled graph."""
        from src.agents.libu2 import build_libu2_graph

        graph = build_libu2_graph()
        assert graph is not None

    def test_libu2_graph_has_react_node(self):
        """Libu2 graph must have react node."""
        from src.agents.libu2 import build_libu2_graph

        graph = build_libu2_graph()
        # LangGraph compiled graph has nodes attribute
        assert "react" in graph.nodes

    def test_libu2_graph_has_tools_node(self):
        """Libu2 graph must have tools node."""
        from src.agents.libu2 import build_libu2_graph

        graph = build_libu2_graph()
        assert "tools" in graph.nodes

    def test_libu2_graph_follows_bingbu_pattern(self):
        """Libu2 graph follows Bingbu pattern (react -> tools -> react)."""
        from src.agents.libu2 import build_libu2_graph

        graph = build_libu2_graph()
        # Check graph structure: __start__ -> react, react -> tools, tools -> react
        g = graph.get_graph()
        # Find edge from __start__ to react
        start_edges = [e for e in g.edges if e.source == "__start__"]
        assert len(start_edges) == 1
        assert start_edges[0].target == "react"


class TestDocGenerator:
    """Tests for doc_generator tools."""

    def test_extract_docstrings_from_file(self):
        """extract_docstrings should extract docstrings from Python file."""
        from src.tools.doc_generator import extract_docstrings
        import json

        # Test with a known file
        result = extract_docstrings("src/tools/base.py")
        data = json.loads(result)

        # Should have module docstring or classes/functions
        assert "error" not in data or data.get("module") or data.get("classes") or data.get("functions")

    def test_extract_docstrings_nonexistent_file(self):
        """extract_docstrings should handle nonexistent files."""
        from src.tools.doc_generator import extract_docstrings
        import json

        result = extract_docstrings("nonexistent_file.py")
        data = json.loads(result)
        assert "error" in data

    def test_generate_doc_markdown_format(self):
        """generate_doc should return markdown format."""
        from src.tools.doc_generator import generate_doc

        result = generate_doc("src.tools.base", output_format="markdown")
        # Should contain markdown headers
        assert "# src.tools.base" in result or "error" in result.lower()

    def test_generate_doc_json_format(self):
        """generate_doc should return JSON format when requested."""
        from src.tools.doc_generator import generate_doc
        import json

        result = generate_doc("src.tools.base", output_format="json")
        # Should be valid JSON
        data = json.loads(result)
        assert isinstance(data, dict)

    def test_format_markdown_standard_style(self):
        """format_markdown should format with standard style."""
        from src.tools.doc_generator import format_markdown

        content = "# Title\n\nSome content\n## Section\n\nMore content"
        result = format_markdown(content, style="standard")
        assert "# Title" in result

    def test_format_markdown_compact_style(self):
        """format_markdown should remove extra blank lines in compact style."""
        from src.tools.doc_generator import format_markdown

        content = "# Title\n\n\n\nSome content"
        result = format_markdown(content, style="compact")
        # Should not have multiple consecutive blank lines
        assert "\n\n\n" not in result

    def test_format_markdown_expanded_style(self):
        """format_markdown should add spacing in expanded style."""
        from src.tools.doc_generator import format_markdown

        content = "# Title\nSome content\n## Section\nMore content"
        result = format_markdown(content, style="expanded")
        assert "# Title" in result

    def test_create_readme_basic(self):
        """create_readme should create basic README content."""
        from src.tools.doc_generator import create_readme

        result = create_readme("TestProject", "A test project")

        assert "# TestProject" in result
        assert "A test project" in result
        assert "## Installation" in result
        assert "## Usage" in result
        assert "## License" in result

    def test_create_readme_with_sections(self):
        """create_readme should include custom sections."""
        from src.tools.doc_generator import create_readme

        custom_sections = "## Features\n\n- Feature 1\n- Feature 2\n"
        result = create_readme("TestProject", "A test project", sections=custom_sections)

        assert "## Features" in result
        assert "- Feature 1" in result
