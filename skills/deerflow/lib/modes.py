"""Mode preset definitions and DeerFlowClient config mapping.

This module provides mode presets for deer-flow agent invocation:

- flash: thinking=false, plan_mode=false, subagent=false (fastest)
- standard: thinking=true, plan_mode=false, subagent=false (default)
- pro: thinking=true, plan_mode=true, subagent=false
- ultra: thinking=true, plan_mode=true, subagent=true

Usage:
    from lib.modes import get_mode_config, MODE_PRESETS

    # Get kwargs for DeerFlowClient constructor
    client_kwargs = get_mode_config("standard")
    client = DeerFlowClient(config_path=path, **client_kwargs)
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class ModeConfig:
    """Configuration for a mode preset.

    Maps to DeerFlowClient constructor parameters:
    - thinking_enabled: Extended thinking mode
    - plan_mode: Planning mode for complex tasks
    - subagent_enabled: Subagent delegation capability
    """

    thinking_enabled: bool
    plan_mode: bool
    subagent_enabled: bool


# Mode preset definitions matching CONTEXT.md decisions exactly
MODE_PRESETS: dict[str, ModeConfig] = {
    "flash": ModeConfig(
        thinking_enabled=False, plan_mode=False, subagent_enabled=False
    ),
    "standard": ModeConfig(
        thinking_enabled=True, plan_mode=False, subagent_enabled=False
    ),
    "pro": ModeConfig(
        thinking_enabled=True, plan_mode=True, subagent_enabled=False
    ),
    "ultra": ModeConfig(
        thinking_enabled=True, plan_mode=True, subagent_enabled=True
    ),
}


def get_mode_config(mode: str = "standard") -> dict:
    """Get DeerFlowClient kwargs for a mode preset.

    Args:
        mode: Mode name (flash, standard, pro, ultra). Default is standard.

    Returns:
        Dict of kwargs for DeerFlowClient constructor:
        {
            "thinking_enabled": bool,
            "plan_mode": bool,
            "subagent_enabled": bool,
        }

    Raises:
        ValueError: If mode name is not recognized.
    """
    if mode not in MODE_PRESETS:
        available_modes = list(MODE_PRESETS.keys())
        raise ValueError(
            f"Unknown mode: '{mode}'. Available modes: {available_modes}"
        )

    config = MODE_PRESETS[mode]
    return {
        "thinking_enabled": config.thinking_enabled,
        "plan_mode": config.plan_mode,
        "subagent_enabled": config.subagent_enabled,
    }