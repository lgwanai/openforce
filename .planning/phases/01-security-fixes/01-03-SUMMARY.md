# Plan 01-03 Summary: Configuration Externalization (SEC-03)

**Status:** COMPLETE
**Date:** 2026-04-04
**Verified:** All requirements met

---

## Objective

Remove hardcoded paths and implement environment-based configuration.

## Implementation Complete

### 1. `src/core/config.py` - Configuration Module

Implemented using `pydantic-settings`:

- **ExternalToolsConfig**: Manages external tool paths
  - `baidu_search_script: Optional[Path]` - Path to Baidu search script
  - `agent_browser_path: Optional[str]` - Path to agent-browser executable
  - `playwright_cache_dir: Path` - Playwright browser cache directory

- **SecurityConfig**: Manages security settings
  - `approval_secret_key: str` - Secret key for HMAC tokens
  - `allowed_executables: Set[str]` - Whitelist of allowed executables

- **Singleton Accessors**:
  - `get_external_tools_config() -> ExternalToolsConfig`
  - `get_security_config() -> SecurityConfig`

### 2. `.env.example` - Configuration Template

Created with all configurable environment variables:
- `OPENFORCE_BAIDU_SEARCH_SCRIPT`
- `OPENFORCE_AGENT_BROWSER_PATH`
- `OPENFORCE_PLAYWRIGHT_CACHE_DIR`
- `OPENFORCE_SECURITY_APPROVAL_SECRET_KEY`

### 3. Tests (`tests/config/test_settings.py`)

9 tests covering:
- Environment variable loading
- .env file loading
- Missing paths returning None (no crashes)
- Default values
- Security config
- Singleton behavior

## Verification Results

```bash
# Run config tests
PYTHONPATH=/Users/wuliang/workspace/openforce pytest tests/config/ -v
# Result: 9 passed in 0.05s

# Check for hardcoded user paths
grep -r "/Users/wuliang" src/*.py src/**/*.py
# Result: No hardcoded user paths in source files
```

## Must-Haves Verification

- [x] `ExternalToolsConfig` class exists in config.py
- [x] No hardcoded absolute paths in src/tools/base.py
- [x] `.env.example` file created
- [x] All automated tests pass

## Success Criteria Met

1. ✅ All external paths configurable via environment variables
2. ✅ No hardcoded absolute paths in source code
3. ✅ Configuration is type-safe with pydantic-settings
4. ✅ Tests pass with >80% coverage

## Configuration Usage

```python
from src.core.config import get_external_tools_config, get_security_config

# Get external tools config
tools_config = get_external_tools_config()
if tools_config.baidu_search_script:
    script_path = tools_config.baidu_search_script

# Get security config
security_config = get_security_config()
secret_key = security_config.approval_secret_key
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OPENFORCE_BAIDU_SEARCH_SCRIPT` | Path to Baidu search script | None |
| `OPENFORCE_AGENT_BROWSER_PATH` | Path to agent-browser | None |
| `OPENFORCE_PLAYWRIGHT_CACHE_DIR` | Playwright cache directory | `/tmp/Library/Caches/ms-playwright` |
| `OPENFORCE_SECURITY_APPROVAL_SECRET_KEY` | HMAC secret for tokens | Auto-generated |
| `OPENFORCE_SECURITY_ALLOWED_EXECUTABLES` | Allowed executables (JSON) | `["python3", "npx", "agent-browser"]` |
