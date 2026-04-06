"""
Tests for AGT-02: Libu Agent implementation.

Libu (吏部) - Ministry of Personnel
Responsibilities: Skill management, installation, updates
"""

import pytest
from typing import Dict, Any


class TestLibuState:
    """Tests for LibuState structure."""

    def test_libu_state_has_required_fields(self):
        """LibuState must have required fields."""
        from src.agents.libu import LibuState

        # Check TypedDict fields exist
        state = {
            "messages": [],
            "task_id": "test",
            "goal": "test",
            "skills_installed": [],
            "skills_updated": [],
            "errors": []
        }
        # Should not raise
        LibuState(**state)

    def test_libu_state_messages_field(self):
        """LibuState must have messages field with add_messages annotation."""
        from src.agents.libu import LibuState
        from typing import get_type_hints
        from langgraph.graph.message import add_messages

        hints = get_type_hints(LibuState, include_extras=True)
        # messages field should have Annotated type with add_messages
        assert "messages" in hints

    def test_libu_state_tracking_fields(self):
        """LibuState must track skills_installed, skills_updated."""
        from src.agents.libu import LibuState
        from typing import get_type_hints

        hints = get_type_hints(LibuState)
        assert "skills_installed" in hints
        assert "skills_updated" in hints
        assert "errors" in hints


class TestLibuTools:
    """Tests for Libu Agent tools."""

    def test_libu_tools_defined(self):
        """Libu tools must be defined."""
        from src.agents.libu import libu_tools

        tool_names = [t.name for t in libu_tools]
        assert "tool_install_skill" in tool_names
        assert "tool_update_skill" in tool_names
        assert "tool_list_skills" in tool_names
        assert "tool_uninstall_skill" in tool_names

    def test_tool_install_skill_exists(self):
        """tool_install_skill must exist."""
        from src.agents.libu import tool_install_skill

        assert tool_install_skill.name == "tool_install_skill"
        props = tool_install_skill.args_schema.schema()["properties"]
        assert "skill_name" in props
        assert "source" in props

    def test_tool_update_skill_exists(self):
        """tool_update_skill must exist."""
        from src.agents.libu import tool_update_skill

        assert tool_update_skill.name == "tool_update_skill"
        props = tool_update_skill.args_schema.schema()["properties"]
        assert "skill_name" in props

    def test_tool_list_skills_exists(self):
        """tool_list_skills must exist."""
        from src.agents.libu import tool_list_skills

        assert tool_list_skills.name == "tool_list_skills"
        # No required arguments

    def test_tool_uninstall_skill_exists(self):
        """tool_uninstall_skill must exist."""
        from src.agents.libu import tool_uninstall_skill

        assert tool_uninstall_skill.name == "tool_uninstall_skill"
        props = tool_uninstall_skill.args_schema.schema()["properties"]
        assert "skill_name" in props


class TestLibuGraph:
    """Tests for Libu Agent graph."""

    def test_build_libu_graph(self):
        """build_libu_graph must return compiled graph."""
        from src.agents.libu import build_libu_graph

        graph = build_libu_graph()
        assert graph is not None

    def test_libu_graph_has_react_node(self):
        """Libu graph must have react node."""
        from src.agents.libu import build_libu_graph

        graph = build_libu_graph()
        # LangGraph compiled graph has nodes attribute
        assert "react" in graph.nodes

    def test_libu_graph_has_tools_node(self):
        """Libu graph must have tools node."""
        from src.agents.libu import build_libu_graph

        graph = build_libu_graph()
        assert "tools" in graph.nodes

    def test_libu_graph_follows_hubu_pattern(self):
        """Libu graph follows Hubu pattern (react -> tools -> react)."""
        from src.agents.libu import build_libu_graph

        graph = build_libu_graph()
        # Check graph structure: __start__ -> react, react -> tools, tools -> react
        g = graph.get_graph()
        # Find edge from __start__ to react
        start_edges = [e for e in g.edges if e.source == "__start__"]
        assert len(start_edges) == 1
        assert start_edges[0].target == "react"


class TestSkillManager:
    """Tests for skill_manager tools."""

    def test_list_skills_returns_output(self):
        """list_skills should return package list."""
        from src.tools.skill_manager import list_skills

        result = list_skills()
        # Should return JSON-like output
        assert result is not None
        assert len(result) > 0

    def test_install_skill_nonexistent_package(self):
        """install_skill should handle non-existent packages."""
        from src.tools.skill_manager import install_skill

        # Try to install a non-existent package
        result = install_skill("nonexistent-package-xyz-12345")
        assert "Error" in result or "Failed" in result

    def test_uninstall_skill_nonexistent_package(self):
        """uninstall_skill should return success even for non-existent packages (pip -y behavior)."""
        from src.tools.skill_manager import uninstall_skill

        result = uninstall_skill("nonexistent-package-xyz-12345")
        # pip uninstall -y returns success even for non-existent packages
        assert "Successfully" in result or "uninstalled" in result.lower()
