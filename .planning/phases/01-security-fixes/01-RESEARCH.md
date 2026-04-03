# Phase 1: Security Fixes - Research

**Researched:** 2026-04-03
**Domain:** Python Security - Subprocess, Cryptography, SSRF, Taint Tracking
**Confidence:** HIGH

## Summary

This phase addresses 5 CRITICAL security vulnerabilities identified in the OpenForce codebase. The vulnerabilities span shell injection, weak token generation, hardcoded paths, SSRF, and incomplete taint tracking. All fixes follow Python security best practices with established standard library solutions.

**Primary recommendation:** Use Python standard library (`subprocess` without shell, `secrets`+`hmac` for tokens, `ipaddress` for SSRF) combined with environment-based configuration management via `pydantic-settings`.

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SEC-01 | Shell injection fix - remove shell=True, use parameter list and whitelist | subprocess.run() with list args, shlex.quote() for any dynamic values |
| SEC-02 | Approval token security - HMAC + secret key signed tokens | hmac module with secrets.token_urlsafe() for secrets |
| SEC-03 | Remove hardcoded paths - config file or environment variables | pydantic-settings for type-safe env/config loading |
| SEC-04 | SSRF protection - fetch_webpage URL protocol and hostname validation | urllib.parse.urlparse + ipaddress module for private IP detection |
| SEC-05 | Taint tracking implementation - real taint propagation and validation logic | Trust level enum, source tracking decorator, taint propagation through data flow |

</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `subprocess` | stdlib | Safe process execution | Standard library, well-tested, no shell injection when used correctly |
| `secrets` | stdlib | Cryptographically secure random | Python 3.6+, designed for security tokens |
| `hmac` | stdlib | Message authentication | Standard for token signatures, proven cryptographic primitive |
| `ipaddress` | stdlib | IP address validation | Handles IPv4/IPv6, private range detection |
| `urllib.parse` | stdlib | URL parsing and validation | Standard library, handles edge cases |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `pydantic-settings` | 2.x | Type-safe configuration | Environment variables and config file loading |
| `pydantic` | 2.x | Data validation | Already in project, use for config models |
| `bandit` | latest | SAST security scanning | CI/CD pipeline, pre-commit hooks |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `secrets` | `uuid.uuid4` | uuid4 is not guaranteed cryptographically secure in all Python implementations |
| `pydantic-settings` | `python-dotenv` | dotenv only loads .env, pydantic-settings provides validation and type coercion |
| `ipaddress` module | `validators` library | validators is third-party, ipaddress is stdlib and sufficient |

**Installation:**
```bash
pip install pydantic-settings bandit
```

## Architecture Patterns

### Recommended Project Structure

```
src/
├── security/
│   ├── __init__.py
│   ├── taint_engine.py      # Taint tracking with propagation
│   ├── approval.py          # HMAC-based approval tokens
│   └── ssrf.py              # URL validation utilities
├── config/
│   ├── __init__.py
│   ├── settings.py          # pydantic-settings base settings
│   └── external_tools.py    # External tool path configuration
├── tools/
│   ├── base.py              # Sandbox tools (no shell=True)
│   └── command_executor.py  # Whitelisted command execution
└── tests/
    └── security/
        ├── test_taint_engine.py
        ├── test_approval_tokens.py
        ├── test_ssrf_protection.py
        └── test_command_execution.py
```

### Pattern 1: Safe Subprocess Execution

**What:** Never use `shell=True`. Use argument lists with explicit executable paths.

**When to use:** All subprocess.run() calls.

**Example:**
```python
# Source: Python stdlib best practices, OWASP guidelines
import subprocess
import shutil
from typing import List, Optional

def run_safe_command(
    executable: str,
    args: List[str],
    timeout: int = 60,
    allowed_executables: Optional[set] = None
) -> tuple[int, str, str]:
    """
    Execute a command safely without shell interpretation.
    
    Args:
        executable: Path or name of executable (must be in whitelist)
        args: List of arguments (no shell expansion)
        timeout: Maximum execution time in seconds
        allowed_executables: Set of allowed executable names/paths
    
    Returns:
        Tuple of (return_code, stdout, stderr)
    """
    # Whitelist validation
    if allowed_executables:
        resolved = shutil.which(executable)
        if resolved not in allowed_executables:
            raise SecurityError(f"Executable not in whitelist: {executable}")
    
    # Build argument list - NEVER use shell=True
    cmd = [executable] + args
    
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
        # Do NOT add shell=True
    )
    
    return result.returncode, result.stdout, result.stderr


# For agent-browser specifically:
def run_agent_browser_safe(command_args: List[str]) -> str:
    """Execute agent-browser with argument list (no shell)."""
    # Find executable
    executable = shutil.which("agent-browser")
    if not executable:
        # Fallback to npx
        executable = "npx"
        cmd = ["npx", "agent-browser"] + command_args
    else:
        cmd = [executable] + command_args
    
    returncode, stdout, stderr = run_safe_command(
        executable,
        command_args,
        timeout=60,
        allowed_executables={"/usr/local/bin/agent-browser", "/usr/bin/npx"}
    )
    
    return stdout + stderr
```

### Pattern 2: HMAC-Based Approval Tokens

**What:** Use HMAC with a secret key to generate tamper-proof approval tokens.

**When to use:** Any approval/authorization token generation.

**Example:**
```python
# Source: Python hmac documentation, security best practices
import hmac
import hashlib
import time
import secrets
from typing import Optional

class ApprovalTokenManager:
    """
    HMAC-based approval token generation and verification.
    Tokens are cryptographically signed and time-limited.
    """
    
    def __init__(self, secret_key: Optional[bytes] = None):
        """
        Initialize with secret key.
        If not provided, generate a secure random key (store this!).
        """
        self._secret_key = secret_key or secrets.token_bytes(32)
    
    def generate_token(
        self,
        owner_user_id: str,
        task_id: str,
        approval_id: str,
        action_hash: str,
        expires_in: int = 3600,  # 1 hour default
        nonce: Optional[str] = None
    ) -> str:
        """
        Generate a signed approval token.
        
        Token format: <timestamp>:<nonce>:<hmac_signature>
        """
        exp = int(time.time()) + expires_in
        nonce = nonce or secrets.token_urlsafe(16)
        
        # Build message to sign
        message = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}"
        
        # Generate HMAC signature
        signature = hmac.new(
            self._secret_key,
            message.encode('utf-8'),
            hashlib.sha256
        ).hexdigest()
        
        # Return encoded token
        return f"{exp}:{nonce}:{signature}"
    
    def verify_token(
        self,
        token: str,
        owner_user_id: str,
        task_id: str,
        approval_id: str,
        action_hash: str
    ) -> bool:
        """
        Verify token signature and expiration.
        Uses constant-time comparison to prevent timing attacks.
        """
        try:
            parts = token.split(':')
            if len(parts) != 3:
                return False
            
            exp_str, nonce, provided_signature = parts
            exp = int(exp_str)
            
            # Check expiration
            if time.time() > exp:
                return False
            
            # Recompute signature
            message = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}"
            expected_signature = hmac.new(
                self._secret_key,
                message.encode('utf-8'),
                hashlib.sha256
            ).hexdigest()
            
            # Constant-time comparison to prevent timing attacks
            return hmac.compare_digest(expected_signature, provided_signature)
            
        except (ValueError, TypeError):
            return False


# Usage:
# Store secret key in environment variable or secure config
token_manager = ApprovalTokenManager(
    secret_key=os.environ.get('APPROVAL_SECRET_KEY', '').encode() or None
)
```

### Pattern 3: Environment-Based Configuration

**What:** Use pydantic-settings for type-safe, environment-driven configuration.

**When to use:** All external paths, API keys, and deployment-specific values.

**Example:**
```python
# Source: pydantic-settings documentation
from pydantic_settings import BaseSettings, SettingsConfigDict
from pydantic import Field
from typing import Optional
from pathlib import Path

class ExternalToolsConfig(BaseSettings):
    """Configuration for external tools and paths."""
    
    model_config = SettingsConfigDict(
        env_prefix='OPENFORCE_',  # Environment variables: OPENFORCE_*
        env_file='.env',
        env_file_encoding='utf-8',
        extra='ignore'
    )
    
    # External tool paths
    baidu_search_script: Optional[Path] = Field(
        default=None,
        description='Path to Baidu search skill script'
    )
    
    agent_browser_path: Optional[str] = Field(
        default=None,
        description='Path to agent-browser executable'
    )
    
    playwright_cache_dir: Optional[Path] = Field(
        default=Path('/tmp/Library/Caches/ms-playwright'),
        description='Playwright browser cache directory'
    )
    
    @property
    def agent_browser_executable(self) -> str:
        """Get agent-browser executable, fallback to npx."""
        if self.agent_browser_path and Path(self.agent_browser_path).exists():
            return str(self.agent_browser_path)
        return 'npx'  # Will use npx agent-browser


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
    
    allowed_executables: set[str] = Field(
        default={'python3', 'npx', 'agent-browser'},
        description='Whitelist of allowed executables'
    )


# Usage:
config = ExternalToolsConfig()
security_config = SecurityConfig()

# Access paths:
script_path = config.baidu_search_script  # None if not set, or path from env
```

### Pattern 4: SSRF Protection

**What:** Validate URLs before fetching to prevent Server-Side Request Forgery.

**When to use:** All HTTP requests to user-provided URLs.

**Example:**
```python
# Source: OWASP SSRF Prevention Cheat Sheet, Python stdlib
import ipaddress
import socket
from urllib.parse import urlparse
from typing import Optional
import httpx

class SSRFError(Exception):
    """Raised when SSRF attempt is detected."""
    pass

# Private IP ranges to block
PRIVATE_IP_RANGES = [
    ipaddress.ip_network('10.0.0.0/8'),      # RFC 1918
    ipaddress.ip_network('172.16.0.0/12'),   # RFC 1918
    ipaddress.ip_network('192.168.0.0/16'),  # RFC 1918
    ipaddress.ip_network('127.0.0.0/8'),     # Loopback
    ipaddress.ip_network('169.254.0.0/16'),  # Link-local
    ipaddress.ip_network('::1/128'),         # IPv6 loopback
    ipaddress.ip_network('fc00::/7'),        # IPv6 ULA
    ipaddress.ip_network('fe80::/10'),       # IPv6 link-local
]

ALLOWED_SCHEMES = {'http', 'https'}

BLOCKED_HOSTS = {
    'localhost',
    'localhost.localdomain',
    'ip6-localhost',
    'ip6-loopback',
}


def validate_url_for_ssrf(url: str) -> str:
    """
    Validate URL is safe for server-side fetching.
    
    Returns:
        The validated URL
        
    Raises:
        SSRFError: If URL is potentially malicious
    """
    try:
        parsed = urlparse(url)
    except Exception as e:
        raise SSRFError(f"Invalid URL format: {e}")
    
    # Check scheme
    if parsed.scheme.lower() not in ALLOWED_SCHEMES:
        raise SSRFError(f"URL scheme not allowed: {parsed.scheme}")
    
    # Check for blocked hostnames
    hostname = parsed.hostname
    if not hostname:
        raise SSRFError("URL must have a hostname")
    
    if hostname.lower() in BLOCKED_HOSTS:
        raise SSRFError(f"Hostname not allowed: {hostname}")
    
    # Resolve hostname to IP and check for private ranges
    try:
        # Get all IPs for hostname (handles DNS rebinding)
        addr_infos = socket.getaddrinfo(hostname, parsed.port or 443)
        
        for family, socktype, proto, canonname, sockaddr in addr_infos:
            ip_str = sockaddr[0]
            ip = ipaddress.ip_address(ip_str)
            
            for private_range in PRIVATE_IP_RANGES:
                if ip in private_range:
                    raise SSRFError(
                        f"Resolved IP {ip} is in private range {private_range}"
                    )
                    
    except socket.gaierror as e:
        raise SSRFError(f"Could not resolve hostname: {e}")
    
    return url


def fetch_webpage_safe(url: str, timeout: float = 10.0) -> str:
    """
    Safely fetch webpage content with SSRF protection.
    """
    from bs4 import BeautifulSoup
    
    # Validate URL first
    validated_url = validate_url_for_ssrf(url)
    
    try:
        resp = httpx.get(validated_url, timeout=timeout, follow_redirects=False)
        resp.raise_for_status()
        
        soup = BeautifulSoup(resp.text, "html.parser")
        return soup.get_text(separator="\n", strip=True)[:5000]
        
    except httpx.HTTPStatusError as e:
        return f"HTTP error fetching webpage: {e}"
    except Exception as e:
        return f"Failed to fetch webpage: {str(e)}"
```

### Pattern 5: Taint Tracking Implementation

**What:** Track data provenance and enforce trust-based access controls.

**When to use:** All sensitive operations that handle external data.

**Example:**
```python
# Source: Security taint analysis patterns
from enum import Enum
from typing import Any, Dict, List, Optional, Set
from dataclasses import dataclass, field
from functools import wraps
import hashlib


class TrustLevel(Enum):
    """Trust levels for data provenance."""
    TRUSTED = "trusted"      # Internal system data
    DERIVED = "derived"      # Processed from untrusted, may be sanitized
    UNTRUSTED = "untrusted"  # External input (web, user, upload)


class TaintSource(Enum):
    """Sources of potentially tainted data."""
    INTERNAL = "internal"
    USER_FREE_TEXT = "user_free_text"
    WEB = "web"
    SEARCH = "search"
    UPLOAD = "upload"


@dataclass
class TaintedValue:
    """
    A value with taint metadata.
    Tracks sources and trust level for security decisions.
    """
    value: Any
    sources: Set[TaintSource] = field(default_factory=lambda: {TaintSource.INTERNAL})
    trust_level: TrustLevel = TrustLevel.TRUSTED
    
    def __post_init__(self):
        # Derive trust level from sources
        if TaintSource.WEB in self.sources or TaintSource.UPLOAD in self.sources:
            self.trust_level = TrustLevel.UNTRUSTED
        elif TaintSource.USER_FREE_TEXT in self.sources or TaintSource.SEARCH in self.sources:
            self.trust_level = TrustLevel.DERIVED
    
    def propagate_to(self, new_value: Any) -> 'TaintedValue':
        """Propagate taint to a derived value."""
        return TaintedValue(
            value=new_value,
            sources=self.sources.copy(),
            trust_level=self.trust_level  # Taint propagates
        )
    
    @staticmethod
    def trusted(value: Any) -> 'TaintedValue':
        """Create a trusted internal value."""
        return TaintedValue(value=value, sources={TaintSource.INTERNAL})
    
    @staticmethod
    def from_web(value: Any) -> 'TaintedValue':
        """Create a value from web source."""
        return TaintedValue(value=value, sources={TaintSource.WEB})
    
    @staticmethod
    def from_user(value: Any) -> 'TaintedValue':
        """Create a value from user input."""
        return TaintedValue(value=value, sources={TaintSource.USER_FREE_TEXT})


class TaintEngine:
    """
    Taint propagation and sensitive tool verification.
    """
    
    # High-risk tools that require elevated trust
    HIGH_RISK_TOOLS = frozenset([
        "execute_command",
        "delete_file",
        "write_api",
        "run_shell",
    ])
    
    # Medium-risk tools that accept derived trust
    MEDIUM_RISK_TOOLS = frozenset([
        "write_file",
        "run_script",
    ])
    
    @classmethod
    def check_tool_call(
        cls,
        tool_name: str,
        args: Dict[str, Any],
        tainted_args: Optional[Dict[str, TaintedValue]] = None
    ) -> bool:
        """
        Check if tool call is allowed based on taint level.
        
        Returns:
            True if allowed, False if blocked (requires approval)
        """
        if tool_name not in cls.HIGH_RISK_TOOLS and tool_name not in cls.MEDIUM_RISK_TOOLS:
            return True
        
        # Check if any arguments are tainted
        if tainted_args:
            min_trust = cls._get_minimum_trust(tool_name)
            
            for arg_name, tainted_value in tainted_args.items():
                if tainted_value.trust_level.value > min_trust.value:
                    # Argument has lower trust than required
                    return False
        
        # High-risk tools always require approval
        if tool_name in cls.HIGH_RISK_TOOLS:
            return False
        
        return True
    
    @staticmethod
    def _get_minimum_trust(tool_name: str) -> TrustLevel:
        """Get minimum trust level required for tool."""
        if tool_name in TaintEngine.HIGH_RISK_TOOLS:
            return TrustLevel.TRUSTED
        elif tool_name in TaintEngine.MEDIUM_RISK_TOOLS:
            return TrustLevel.DERIVED
        return TrustLevel.UNTRUSTED
    
    @staticmethod
    def get_trust_level(sources: List[TaintSource]) -> TrustLevel:
        """Determine trust level from data sources."""
        source_set = set(sources)
        
        if TaintSource.WEB in source_set or TaintSource.UPLOAD in source_set:
            return TrustLevel.UNTRUSTED
        if TaintSource.USER_FREE_TEXT in source_set or TaintSource.SEARCH in source_set:
            return TrustLevel.DERIVED
        return TrustLevel.TRUSTED
    
    @staticmethod
    def sanitize(value: TaintedValue, sanitizer: str) -> TaintedValue:
        """
        Sanitize a tainted value and upgrade its trust level.
        
        Args:
            value: The tainted value
            sanitizer: Name of sanitization method (for audit)
        
        Returns:
            New TaintedValue with DERIVED trust level
        """
        return TaintedValue(
            value=value.value,
            sources=value.sources | {TaintSource.INTERNAL},  # Add internal source
            trust_level=TrustLevel.DERIVED  # Upgrade from UNTRUSTED
        )


def taint_source(source: TaintSource):
    """
    Decorator to mark function output as tainted from a specific source.
    """
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            result = func(*args, **kwargs)
            if isinstance(result, TaintedValue):
                return result
            return TaintedValue(value=result, sources={source})
        return wrapper
    return decorator


# Usage examples:
class ToolsWithTaintTracking:
    """Tools that track taint through operations."""
    
    @taint_source(TaintSource.WEB)
    def fetch_webpage(self, url: str) -> TaintedValue:
        """Fetch returns tainted data."""
        # ... fetch logic ...
        return TaintedValue.from_web("web content")
    
    def process_content(self, tainted_content: TaintedValue) -> TaintedValue:
        """Processing propagates taint."""
        processed = tainted_content.value.upper()
        return tainted_content.propagate_to(processed)
    
    def write_file(self, path: str, content: TaintedValue) -> bool:
        """Write checks taint level."""
        engine = TaintEngine()
        
        if not engine.check_tool_call(
            "write_file",
            {"path": path, "content": content.value},
            {"content": content}
        ):
            raise SecurityError(
                "Cannot write untrusted content to file. "
                "Content must be sanitized first."
            )
        
        # Safe to write
        # ... write logic ...
        return True
```

### Anti-Patterns to Avoid

- **Using `shell=True` with user input:** Enables command injection attacks. Always use argument lists.
- **Hashing without secret key:** SHA256 alone is not a MAC. Use HMAC with a secret.
- **Hardcoded paths:** Prevents deployment portability. Use environment variables.
- **URL validation without DNS resolution:** DNS rebinding attacks can bypass hostname checks.
- **Taint tracking that always returns True:** Provides false security assurance.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Shell escaping | `shlex.quote()` everywhere | subprocess with list args | Quote escaping is error-prone, list args are inherently safe |
| Random tokens | `random` module | `secrets` module | `random` is not cryptographically secure |
| Secret storage | Custom encryption | Environment variables + secrets manager | OS-level secret managers have better security |
| IP validation | Regex on IP strings | `ipaddress` module | Handles IPv6, edge cases, CIDR notation |
| URL parsing | String manipulation | `urllib.parse.urlparse` | Handles encoding, edge cases |

**Key insight:** Python's standard library has battle-tested security primitives. Custom implementations introduce subtle vulnerabilities.

## Common Pitfalls

### Pitfall 1: Command Injection via Environment Variables

**What goes wrong:** Even with argument lists, environment variables can be exploited.

**Why it happens:** `subprocess.run` inherits parent environment by default.

**How to avoid:**
```python
# Explicitly set environment
subprocess.run(cmd, env={'PATH': '/usr/bin'}, ...)
```

**Warning signs:** Commands that rely on PATH lookup, user-controlled environment.

### Pitfall 2: Timing Attacks on Token Verification

**What goes wrong:** String comparison leaks information about correct token.

**Why it happens:** Normal `==` short-circuits on first mismatch.

**How to avoid:** Always use `hmac.compare_digest()` for token comparison.

**Warning signs:** Using `token == expected_token` in verification code.

### Pitfall 3: DNS Rebinding for SSRF

**What goes wrong:** Hostname resolves to different IP after initial check.

**Why it happens:** DNS TTL can be set very low by attacker.

**How to avoid:** 
1. Block redirects and re-validate after each redirect
2. Consider using allowlist of domains instead of blocklist
3. Use dedicated SSRF protection libraries for complex cases

**Warning signs:** Following redirects, caching DNS results.

### Pitfall 4: Insufficient Taint Propagation

**What goes wrong:** Taint not tracked through all transformations.

**Why it happens:** Easy to miss implicit data flows.

**How to avoid:** Use taint propagation on every data transformation, default to propagating taint.

**Warning signs:** TaintedValue loses its taint after processing.

### Pitfall 5: Secret Key in Source Code

**What goes wrong:** Secret keys committed to git repository.

**Why it happens:** Convenience during development.

**How to avoid:**
1. Use `.env` files (in `.gitignore`)
2. Generate keys at first run, store securely
3. Use secret managers in production

**Warning signs:** `SECRET_KEY = "hardcoded"` in source files.

## Code Examples

### Safe Command Execution with Whitelist

```python
# Source: OWASP Python Security Cheat Sheet
import subprocess
import shutil
from pathlib import Path

class CommandWhitelist:
    """Whitelist-based command execution."""
    
    def __init__(self):
        self._allowed: dict[str, str] = {}
    
    def allow(self, name: str, path: str | None = None):
        """Add command to whitelist."""
        resolved = shutil.which(path or name)
        if resolved:
            self._allowed[name] = resolved
    
    def run(self, name: str, args: list[str], **kwargs) -> subprocess.CompletedProcess:
        """Execute whitelisted command."""
        if name not in self._allowed:
            raise SecurityError(f"Command not in whitelist: {name}")
        
        cmd = [self._allowed[name]] + args
        return subprocess.run(cmd, shell=False, **kwargs)


# Usage:
whitelist = CommandWhitelist()
whitelist.allow('python3')
whitelist.allow('agent-browser', '/usr/local/bin/agent-browser')

result = whitelist.run('python3', ['--version'], capture_output=True, text=True)
```

### Environment Variable Configuration with Validation

```python
# Source: pydantic-settings documentation
from pydantic_settings import BaseSettings
from pydantic import validator
from pathlib import Path

class AppSettings(BaseSettings):
    """Application settings with environment variable support."""
    
    # Prefix for env vars: OPENFORCE_*
    class Config:
        env_prefix = "OPENFORCE_"
        env_file = ".env"
        env_file_encoding = "utf-8"
    
    # External tool paths
    baidu_search_script: Path | None = None
    agent_browser_path: Path | None = None
    
    # Security settings
    approval_secret_key: str | None = None
    
    @validator('baidu_search_script', 'agent_browser_path', pre=True)
    def validate_path(cls, v):
        if v is None:
            return None
        path = Path(v)
        if not path.exists():
            raise ValueError(f"Path does not exist: {path}")
        return path


# .env file:
# OPENFORCE_BAIDU_SEARCH_SCRIPT=/path/to/search.py
# OPENFORCE_APPROVAL_SECRET_KEY=your-secret-key-here
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `random` module for tokens | `secrets` module | Python 3.6 (2016) | Cryptographically secure random |
| `shell=True` for convenience | Always use list args | Security awareness 2010s | Eliminates shell injection |
| SHA256 hashing for tokens | HMAC with secret key | OWASP guidelines | Prevents token forgery |
| String comparison for secrets | `hmac.compare_digest` | Timing attack awareness 2010s | Prevents timing attacks |
| Blocklist SSRF protection | Allowlist + DNS validation | SSRF research 2015+ | More robust protection |

**Deprecated/outdated:**
- `random` module for security tokens: Use `secrets` module instead
- `shell=True` for subprocess: Always use argument lists
- Simple hostname blocklist: Use DNS resolution + IP range validation

## Open Questions

1. **Agent-browser environment setup**
   - What we know: Current code symlinks Playwright cache to fix permissions
   - What's unclear: Is this the right approach, or should we use a container?
   - Recommendation: Keep current approach but make paths configurable via env

2. **Secret key management**
   - What we know: Need HMAC secret key for tokens
   - What's unclear: Where to store in production (env var, file, secrets manager)
   - Recommendation: Start with environment variable, plan for secrets manager in production

3. **Test coverage for security fixes**
   - What we know: Existing tests are minimal
   - What's unclear: Should we use a security testing framework?
   - Recommendation: Add pytest tests with pytest-cov, integrate bandit in CI

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | pytest |
| Config file | None detected - need to create `pytest.ini` |
| Quick run command | `pytest tests/security/ -x -v` |
| Full suite command | `pytest tests/ --cov=src --cov-report=term-missing` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| SEC-01 | Shell injection blocked | unit | `pytest tests/security/test_command_execution.py::test_shell_injection_blocked -x` | No - Wave 0 |
| SEC-01 | Whitelist enforced | unit | `pytest tests/security/test_command_execution.py::test_non_whitelisted_blocked -x` | No - Wave 0 |
| SEC-02 | Token HMAC signature valid | unit | `pytest tests/security/test_approval_tokens.py::test_token_signature_valid -x` | No - Wave 0 |
| SEC-02 | Token expiration checked | unit | `pytest tests/security/test_approval_tokens.py::test_expired_token_rejected -x` | No - Wave 0 |
| SEC-02 | Timing attack prevented | unit | `pytest tests/security/test_approval_tokens.py::test_timing_attack_resistance -x` | No - Wave 0 |
| SEC-03 | Config loads from env | unit | `pytest tests/config/test_settings.py::test_env_config_loaded -x` | No - Wave 0 |
| SEC-03 | Hardcoded path removed | integration | `pytest tests/tools/test_base.py::test_no_hardcoded_paths -x` | No - Wave 0 |
| SEC-04 | SSRF private IP blocked | unit | `pytest tests/security/test_ssrf.py::test_private_ip_blocked -x` | No - Wave 0 |
| SEC-04 | SSRF localhost blocked | unit | `pytest tests/security/test_ssrf.py::test_localhost_blocked -x` | No - Wave 0 |
| SEC-04 | SSRF valid URL allowed | unit | `pytest tests/security/test_ssrf.py::test_valid_url_allowed -x` | No - Wave 0 |
| SEC-05 | Taint propagates correctly | unit | `pytest tests/security/test_taint_engine.py::test_taint_propagation -x` | No - Wave 0 |
| SEC-05 | High-risk tools blocked | unit | `pytest tests/security/test_taint_engine.py::test_high_risk_blocked -x` | No - Wave 0 |

### Sampling Rate

- **Per task commit:** `pytest tests/security/ -x -v` (fast feedback)
- **Per wave merge:** `pytest tests/ --cov=src --cov-report=term-missing` (comprehensive)
- **Phase gate:** Full suite green + bandit scan clean before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `tests/conftest.py` - shared fixtures (mock settings, token manager)
- [ ] `tests/security/test_command_execution.py` - SEC-01 tests
- [ ] `tests/security/test_approval_tokens.py` - SEC-02 tests
- [ ] `tests/config/test_settings.py` - SEC-03 tests
- [ ] `tests/security/test_ssrf.py` - SEC-04 tests
- [ ] `tests/security/test_taint_engine.py` - SEC-05 tests
- [ ] `pytest.ini` - pytest configuration
- [ ] Framework install: `pip install pytest pytest-cov bandit` - if not in requirements

### Security Regression Tests

Recommended additions to CI/CD pipeline:

```yaml
# .github/workflows/security.yml (future)
- name: Run Bandit SAST
  run: bandit -r src/ -ll

- name: Run Security Tests
  run: pytest tests/security/ -v --cov=src/security
```

## Sources

### Primary (HIGH confidence)

- Python stdlib documentation: `subprocess`, `secrets`, `hmac`, `ipaddress`, `urllib.parse`
- OWASP Python Security Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Python_Security_Cheat_Sheet.html
- OWASP SSRF Prevention Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html

### Secondary (MEDIUM confidence)

- pydantic-settings documentation: https://docs.pydantic.dev/latest/concepts/pydantic_settings/
- Bandit SAST tool: https://github.com/PyCQA/bandit

### Tertiary (LOW confidence)

- Web search results for Python security patterns (verified against primary sources)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Using Python stdlib with proven security primitives
- Architecture: HIGH - Patterns based on OWASP and Python security guidelines
- Pitfalls: HIGH - Common vulnerabilities with known mitigations

**Research date:** 2026-04-03
**Valid until:** 30 days - stdlib patterns are stable, but check for new security advisories
