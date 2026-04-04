import pytest
import os
import tempfile
from pathlib import Path


class TestExternalToolsConfig:
    """Tests for SEC-03: External tools configuration via environment variables."""

    def test_config_loads_from_environment(self, monkeypatch):
        """Verify config loads from environment variables with OPENFORCE_ prefix."""
        from src.core.config import ExternalToolsConfig

        monkeypatch.setenv('OPENFORCE_BAIDU_SEARCH_SCRIPT', '/tmp/test_search.py')
        monkeypatch.setenv('OPENFORCE_AGENT_BROWSER_PATH', '/usr/local/bin/agent-browser')

        config = ExternalToolsConfig()

        assert config.baidu_search_script == Path('/tmp/test_search.py')
        assert config.agent_browser_path == '/usr/local/bin/agent-browser'

    def test_config_loads_from_dotenv_file(self, tmp_path):
        """Verify config loads from .env file."""
        from src.core.config import ExternalToolsConfig

        # Create a temporary .env file
        env_file = tmp_path / '.env'
        env_file.write_text('OPENFORCE_BAIDU_SEARCH_SCRIPT=/custom/search.py\n')

        # Create config with custom env_file path
        config = ExternalToolsConfig(_env_file=str(env_file))

        assert config.baidu_search_script == Path('/custom/search.py')

    def test_missing_paths_return_none(self):
        """Verify missing paths return None, not crash."""
        from src.core.config import ExternalToolsConfig

        # Clear any environment variables
        with pytest.MonkeyPatch.context() as m:
            m.delenv('OPENFORCE_BAIDU_SEARCH_SCRIPT', raising=False)
            m.delenv('OPENFORCE_AGENT_BROWSER_PATH', raising=False)

            config = ExternalToolsConfig()

            assert config.baidu_search_script is None
            assert config.agent_browser_path is None

    def test_default_playwright_cache_dir(self):
        """Verify playwright_cache_dir has a default value."""
        from src.core.config import ExternalToolsConfig

        config = ExternalToolsConfig()

        # Default should be a path, not None
        assert config.playwright_cache_dir is not None
        assert isinstance(config.playwright_cache_dir, Path)


class TestSecurityConfig:
    """Tests for SEC-03: Security configuration."""

    def test_security_config_approval_secret_key(self):
        """Verify security config has approval_secret_key."""
        from src.core.config import SecurityConfig

        config = SecurityConfig()

        # Should auto-generate a secret key
        assert config.approval_secret_key is not None
        assert len(config.approval_secret_key) > 20

    def test_security_config_allowed_executables(self):
        """Verify security config has default allowed executables."""
        from src.core.config import SecurityConfig

        config = SecurityConfig()

        assert 'python3' in config.allowed_executables
        assert 'npx' in config.allowed_executables

    def test_security_config_env_override(self, monkeypatch):
        """Verify security config can be overridden via environment."""
        from src.core.config import SecurityConfig

        monkeypatch.setenv('OPENFORCE_SECURITY_APPROVAL_SECRET_KEY', 'my-custom-secret-key')

        config = SecurityConfig()

        assert config.approval_secret_key == 'my-custom-secret-key'


class TestGetSettings:
    """Tests for settings accessor functions."""

    def test_get_external_tools_config_returns_singleton(self):
        """Verify get_external_tools_config returns a singleton."""
        from src.core.config import get_external_tools_config

        # Clear singleton
        import src.core.config as config_module
        config_module._external_tools_config = None

        config1 = get_external_tools_config()
        config2 = get_external_tools_config()

        assert config1 is config2

    def test_get_security_config_returns_singleton(self):
        """Verify get_security_config returns a singleton."""
        from src.core.config import get_security_config

        # Clear singleton
        import src.core.config as config_module
        config_module._security_config = None

        config1 = get_security_config()
        config2 = get_security_config()

        assert config1 is config2
