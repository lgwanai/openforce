import os
import yaml
import secrets
from pydantic import BaseModel, Field
from pydantic_settings import BaseSettings, SettingsConfigDict
from typing import Dict, List, Optional, Set
from pathlib import Path
from dotenv import load_dotenv

# Load .env file from project root (find .env by walking up from current file)
_project_root = Path(__file__).resolve().parent.parent.parent
_env_file = _project_root / ".env"
load_dotenv(_env_file)

# ============================================================================
# Security & External Tools Configuration (SEC-03)
# ============================================================================

class ExternalToolsConfig(BaseSettings):
    """Configuration for external tools and paths."""

    model_config = SettingsConfigDict(
        env_prefix='OPENFORCE_',
        env_file='.env',
        env_file_encoding='utf-8',
        extra='ignore'
    )

    baidu_search_script: Optional[Path] = Field(
        default=None,
        description='Path to Baidu search skill script'
    )
    agent_browser_path: Optional[str] = Field(
        default=None,
        description='Path to agent-browser executable'
    )
    playwright_cache_dir: Path = Field(
        default=Path('/tmp/Library/Caches/ms-playwright'),
        description='Playwright browser cache directory'
    )


class SecurityConfig(BaseSettings):
    """Security-related configuration."""

    model_config = SettingsConfigDict(
        env_prefix='OPENFORCE_SECURITY_',
        env_file='.env',
        extra='ignore'
    )

    approval_secret_key: str = Field(
        default_factory=lambda: secrets.token_urlsafe(32),
        description='Secret key for approval token HMAC'
    )
    allowed_executables: Set[str] = Field(
        default={'python3', 'npx', 'agent-browser'},
        description='Whitelist of allowed executables'
    )


# Singleton instances
_external_tools_config: Optional[ExternalToolsConfig] = None
_security_config: Optional[SecurityConfig] = None


def get_external_tools_config() -> ExternalToolsConfig:
    """Get the external tools configuration singleton."""
    global _external_tools_config
    if _external_tools_config is None:
        _external_tools_config = ExternalToolsConfig()
    return _external_tools_config


def get_security_config() -> SecurityConfig:
    """Get the security configuration singleton."""
    global _security_config
    if _security_config is None:
        _security_config = SecurityConfig()
    return _security_config


# ============================================================================
# LLM Provider Configuration (Legacy)
# ============================================================================

class ProviderConfig(BaseModel):
    base_url: str
    api_key_env: str

class ModelConfig(BaseModel):
    provider: str
    model: str

class AgentBindingConfig(BaseModel):
    primary: str
    fallbacks: List[str] = []

class AppConfig(BaseModel):
    llm_providers: Dict[str, ProviderConfig]
    llm_models: Dict[str, ModelConfig]
    agent_model_bindings: Dict[str, AgentBindingConfig]

def load_config(config_path: str = "config/models.yaml") -> AppConfig:
    with open(config_path, "r", encoding="utf-8") as f:
        data = yaml.safe_load(f)
    return AppConfig(**data)

def get_llm(agent_role: str, config: Optional[AppConfig] = None):
    if config is None:
        config = load_config()
    
    binding = config.agent_model_bindings.get(agent_role)
    if not binding:
        raise ValueError(f"No binding found for agent role: {agent_role}")
    
    primary_model_key = binding.primary
    model_cfg = config.llm_models.get(primary_model_key)
    if not model_cfg:
        raise ValueError(f"Model config not found for key: {primary_model_key}")
    
    provider_cfg = config.llm_providers.get(model_cfg.provider)
    if not provider_cfg:
        raise ValueError(f"Provider config not found for key: {model_cfg.provider}")
    
    api_key = os.environ.get(provider_cfg.api_key_env)
    if not api_key:
        # Fallback: User might have hardcoded the API key directly in models.yaml
        api_key = provider_cfg.api_key_env

    # Depending on provider, return Langchain ChatModel
    if model_cfg.provider == "anthropic":
        from langchain_anthropic import ChatAnthropic
        return ChatAnthropic(
            model=model_cfg.model,
            anthropic_api_url=provider_cfg.base_url,
            anthropic_api_key=api_key or "dummy",
            temperature=0.2
        )
    else:
        # Default to OpenAI compatible provider for all other custom providers
        from langchain_openai import ChatOpenAI
        return ChatOpenAI(
            model=model_cfg.model,
            base_url=provider_cfg.base_url,
            api_key=api_key or "dummy",
            temperature=0.2
        )
