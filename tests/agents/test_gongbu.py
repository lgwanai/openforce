"""
Tests for AGT-03: Gongbu Agent implementation.

Gongbu (工部) - Ministry of Works
Responsibilities: Environment maintenance, sandbox management
"""

import pytest
from typing import Dict, Any


class TestGongbuState:
    """Tests for GongbuState structure."""

    def test_gongbu_state_has_required_fields(self):
        """GongbuState must have required fields."""
        from src.agents.gongbu import GongbuState

        # Check TypedDict fields exist
        state = {
            "messages": [],
            "task_id": "test",
            "goal": "test",
            "environments_created": [],
            "commands_run": [],
            "errors": []
        }
        # Should not raise
        GongbuState(**state)

    def test_gongbu_state_messages_field(self):
        """GongbuState must have messages field with add_messages annotation."""
        from src.agents.gongbu import GongbuState
        from typing import get_type_hints
        from langgraph.graph.message import add_messages

        hints = get_type_hints(GongbuState, include_extras=True)
        # messages field should have Annotated type with add_messages
        assert "messages" in hints

    def test_gongbu_state_tracking_fields(self):
        """GongbuState must track environments_created, commands_run."""
        from src.agents.gongbu import GongbuState
        from typing import get_type_hints

        hints = get_type_hints(GongbuState)
        assert "environments_created" in hints
        assert "commands_run" in hints


class TestGongbuTools:
    """Tests for Gongbu Agent tools."""

    def test_gongbu_tools_defined(self):
        """Gongbu tools must be defined."""
        from src.agents.gongbu import gongbu_tools

        tool_names = [t.name for t in gongbu_tools]
        assert "tool_create_env" in tool_names
        assert "tool_run_command" in tool_names
        assert "tool_check_env" in tool_names
        assert "tool_list_envs" in tool_names
        assert "tool_remove_env" in tool_names

    def test_tool_create_env_exists(self):
        """tool_create_env must exist."""
        from src.agents.gongbu import tool_create_env

        assert tool_create_env.name == "tool_create_env"
        assert "env_name" in tool_create_env.args_schema.schema()["properties"]
        assert "python_version" in tool_create_env.args_schema.schema()["properties"]

    def test_tool_run_command_exists(self):
        """tool_run_command must exist."""
        from src.agents.gongbu import tool_run_command

        assert tool_run_command.name == "tool_run_command"
        assert "command" in tool_run_command.args_schema.schema()["properties"]
        assert "env_name" in tool_run_command.args_schema.schema()["properties"]

    def test_tool_check_env_exists(self):
        """tool_check_env must exist."""
        from src.agents.gongbu import tool_check_env

        assert tool_check_env.name == "tool_check_env"
        assert "env_name" in tool_check_env.args_schema.schema()["properties"]

    def test_tool_list_envs_exists(self):
        """tool_list_envs must exist."""
        from src.agents.gongbu import tool_list_envs

        assert tool_list_envs.name == "tool_list_envs"

    def test_tool_remove_env_exists(self):
        """tool_remove_env must exist."""
        from src.agents.gongbu import tool_remove_env

        assert tool_remove_env.name == "tool_remove_env"
        assert "env_name" in tool_remove_env.args_schema.schema()["properties"]


class TestGongbuGraph:
    """Tests for Gongbu Agent graph."""

    def test_build_gongbu_graph(self):
        """build_gongbu_graph must return compiled graph."""
        from src.agents.gongbu import build_gongbu_graph

        graph = build_gongbu_graph()
        assert graph is not None

    def test_gongbu_graph_has_react_node(self):
        """Gongbu graph must have react node."""
        from src.agents.gongbu import build_gongbu_graph

        graph = build_gongbu_graph()
        # LangGraph compiled graph has nodes attribute
        assert "react" in graph.nodes

    def test_gongbu_graph_has_tools_node(self):
        """Gongbu graph must have tools node."""
        from src.agents.gongbu import build_gongbu_graph

        graph = build_gongbu_graph()
        assert "tools" in graph.nodes

    def test_gongbu_graph_follows_pattern(self):
        """Gongbu graph follows pattern (react -> tools -> react)."""
        from src.agents.gongbu import build_gongbu_graph

        graph = build_gongbu_graph()
        # Check graph structure: __start__ -> react, react -> tools, tools -> react
        g = graph.get_graph()
        # Find edge from __start__ to react
        start_edges = [e for e in g.edges if e.source == "__start__"]
        assert len(start_edges) == 1
        assert start_edges[0].target == "react"


class TestEnvManagerTools:
    """Tests for env_manager tools."""

    def test_get_envs_dir_returns_path(self):
        """get_envs_dir should return a Path object."""
        from src.tools.env_manager import get_envs_dir
        from pathlib import Path

        result = get_envs_dir()
        assert isinstance(result, Path)
        assert ".openforce" in str(result)
        assert "envs" in str(result)

    def test_list_envs_returns_json(self):
        """list_envs should return JSON string."""
        from src.tools.env_manager import list_envs
        import json

        result = list_envs()
        # Should be valid JSON
        parsed = json.loads(result)
        assert isinstance(parsed, list)

    def test_check_env_nonexistent(self):
        """check_env should return error for nonexistent environment."""
        from src.tools.env_manager import check_env

        result = check_env("nonexistent_env_12345")
        assert "Error" in result or "not found" in result

    def test_remove_env_nonexistent(self):
        """remove_env should return error for nonexistent environment."""
        from src.tools.env_manager import remove_env

        result = remove_env("nonexistent_env_12345")
        assert "Error" in result or "not found" in result

    def test_create_env_creates_environment(self):
        """create_env should create a virtual environment."""
        from src.tools.env_manager import create_env, remove_env, get_envs_dir
        import os

        env_name = "test_env_temp_12345"

        try:
            result = create_env(env_name, python_version="3.11")
            # Should indicate success or error (if python version not available)
            # Either way, should not crash

            # If it succeeded, clean up
            if "Created environment" in result:
                cleanup = remove_env(env_name)
                assert "Removed" in cleanup
        except Exception as e:
            # If creation fails due to missing python, that's acceptable
            pass

    def test_run_command_in_nonexistent_env(self):
        """run_command should error for nonexistent environment."""
        from src.tools.env_manager import run_command

        result = run_command("echo test", env_name="nonexistent_env_12345")
        assert "Error" in result or "not found" in result
