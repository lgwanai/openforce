"""Configuration path resolution and validation.

This module provides functions to resolve the configuration file path,
validate configuration content, and create example configuration templates.

Resolution order (exact):
1. DEER_FLOW_CONFIG_PATH environment variable
2. ./config.yaml (current working directory)
3. ../config.yaml (parent directory)

Usage:
    from lib.config import resolve_and_validate_config

    config_path = resolve_and_validate_config()
"""

import os
from pathlib import Path

# Example config template content
EXAMPLE_CONFIG_TEMPLATE = """# DeerFlow Skill Configuration
# Copy this file to config.yaml and configure your models

models:
  # OpenAI model example
  - name: gpt-4
    use: langchain_openai:ChatOpenAI
    api_key: "$OPENAI_API_KEY"
    model_kwargs:
      model: gpt-4

  # Anthropic model example
  - name: claude-3-sonnet
    use: langchain_anthropic:ChatAnthropic
    api_key: "$ANTHROPIC_API_KEY"
    model_kwargs:
      model: claude-sonnet-4-20250514

  # Ollama model example (optional, uncomment to use)
  # - name: llama3
  #   use: langchain_ollama:ChatOllama
  #   model_kwargs:
  #     model: llama3

sandbox:
  enabled: false  # Skill runs embedded in Claude Code, no sandbox

# To set up credentials:
# 1. Set environment variables:
#    export OPENAI_API_KEY=sk-your-key-here
#    export ANTHROPIC_API_KEY=sk-ant-your-key-here
#
# 2. Or use a .env file with python-dotenv
"""


def resolve_config_path() -> Path:
    """Resolve config.yaml path in priority order.

    Resolution order:
    1. DEER_FLOW_CONFIG_PATH environment variable
    2. ./config.yaml (current working directory)
    3. ../config.yaml (parent directory)

    Returns:
        Path to the config.yaml file.

    Raises:
        FileNotFoundError: If config.yaml is not found at any location.
            Creates config.example.yaml template before raising.
    """
    # 1. Check environment variable
    env_path = os.getenv("DEER_FLOW_CONFIG_PATH")
    if env_path:
        path = Path(env_path)
        if path.exists():
            return path
        # If DEER_FLOW_CONFIG_PATH is set but file doesn't exist,
        # still raise error (user explicitly set a path that's wrong)
        create_example_config()
        raise FileNotFoundError(
            f"Config not found at DEER_FLOW_CONFIG_PATH: {path}\n"
            "Created config.example.yaml with template."
        )

    # 2. Check current directory
    cwd_config = Path.cwd() / "config.yaml"
    if cwd_config.exists():
        return cwd_config

    # 3. Check parent directory
    parent_config = Path.cwd().parent / "config.yaml"
    if parent_config.exists():
        return parent_config

    # Not found - create example template and raise
    create_example_config()
    raise FileNotFoundError(
        "config.yaml not found. Created config.example.yaml with template.\n"
        "Please copy config.example.yaml to config.yaml and configure your models."
    )


def create_example_config() -> None:
    """Create config.example.yaml template in current directory.

    Creates an example configuration file with model examples for
    OpenAI, Anthropic, and Ollama providers.
    """
    example_path = Path.cwd() / "config.example.yaml"
    example_path.write_text(EXAMPLE_CONFIG_TEMPLATE)


def validate_config(config_path: Path) -> None:
    """Validate that config is parseable and has required credentials.

    Args:
        config_path: Path to the config.yaml file.

    Raises:
        ValueError: If config cannot be parsed or credentials are missing.
    """
    import yaml

    # Read and parse YAML
    try:
        content = config_path.read_text()
        config = yaml.safe_load(content)
    except yaml.YAMLError as e:
        raise ValueError(
            f"Failed to parse config.yaml: {e}\n\n"
            "Example config.yaml:\n"
            "models:\n"
            "  - name: gpt-4\n"
            "    use: langchain_openai:ChatOpenAI\n"
            "    api_key: \"$OPENAI_API_KEY\""
        ) from e

    # Check that models section exists
    if not config:
        raise ValueError(
            "config.yaml is empty.\n"
            "Please add model configurations. See config.example.yaml for examples."
        )

    if "models" not in config:
        raise ValueError(
            "config.yaml missing 'models' section.\n"
            "Please add model configurations. See config.example.yaml for examples."
        )

    models = config.get("models", [])
    if not models:
        raise ValueError(
            "config.yaml has empty 'models' section.\n"
            "Please add at least one model configuration. See config.example.yaml for examples."
        )

    # Validate credentials
    missing_credentials = []
    for model in models:
        name = model.get("name", "unknown")
        use = model.get("use", "")
        api_key = model.get("api_key", "")

        # Ollama models don't require api_key
        if "ollama" in use.lower():
            continue

        # Check if api_key is empty or still has unexpanded env var
        if not api_key:
            missing_credentials.append(f"{name}: api_key is empty")
        elif api_key.startswith("$"):
            # Env var wasn't expanded - extract the var name for guidance
            var_name = api_key[1:]  # Remove $ prefix
            missing_credentials.append(f"{name}: {var_name} not set")

    if missing_credentials:
        missing_list = "\n".join(f"  - {m}" for m in missing_credentials)
        raise ValueError(
            f"Missing required credentials:\n{missing_list}\n\n"
            "Example config.yaml:\n"
            "  models:\n"
            "    - name: gpt-4\n"
            "      use: langchain_openai:ChatOpenAI\n"
            "      api_key: \"$OPENAI_API_KEY\"\n\n"
            "Or set in shell:\n"
            "  export OPENAI_API_KEY=sk-...\n"
            "  export ANTHROPIC_API_KEY=sk-ant-..."
        )


def resolve_and_validate_config() -> Path:
    """Resolve config path and validate it.

    Convenience function that combines resolve_config_path() and validate_config().

    Returns:
        Path to the validated config.yaml file.

    Raises:
        FileNotFoundError: If config.yaml is not found.
        ValueError: If config is invalid or credentials are missing.
    """
    config_path = resolve_config_path()
    validate_config(config_path)
    return config_path
