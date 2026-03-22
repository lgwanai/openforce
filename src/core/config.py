import os
import yaml
from pydantic import BaseModel
from typing import Dict, List, Optional

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
