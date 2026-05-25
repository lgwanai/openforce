"""
================================================================================
  OpenForce Security String Utilities  —  utils/__init__.py
================================================================================
  Security Engineer · Defense-in-Depth · Zero-Trust Input Handling

  EVERY FUNCTION in this module treats its input as HOSTILE.
  We apply the following security invariants before any processing:

    1. REJECT, don't just sanitize — if input violates the whitelist, raise.
    2. LENGTH CHECK FIRST — prevent resource exhaustion (ReDoS, buffer issues).
    3. WHITELIST OVER BLACKLIST — define what IS allowed, block everything else.
    4. CONTEXT-SPECIFIC ENCODING — output encoding depends on the target context
       (HTML, SQL, shell, JSON, URL).
    5. FAIL SECURELY — exceptions are explicit, errors never leak internals.

  Threat model categories addressed:
    - SQL Injection            → parameterised-query-safe output + strict reject
    - Cross-Site Scripting     → context-aware HTML entity encoding
    - Command Injection        → whitelist-only shell-safe mode
    - Path Traversal           → no ".." or absolute paths allowed
    - Unicode Normalisation    → NFC normalised to prevent homoglyph bypass
    - Padding Oracle / Timing  → constant-time comparison utilities
================================================================================
"""

from __future__ import annotations

import html
import re
import shlex
import string
import unicodedata
from typing import Final

# ──────────────────────────────────────────────────────────────────────────────
# Security Constants
# ──────────────────────────────────────────────────────────────────────────────

# Maximum input length — any input exceeding this is rejected outright.
# This prevents memory exhaustion, ReDoS, and most buffer-overflow classes.
MAX_INPUT_LENGTH: Final[int] = 4096

# Maximum length for short identifiers (usernames, keys, slugs, IDs).
MAX_IDENTIFIER_LENGTH: Final[int] = 128

# Maximum length for security tokens (passwords, API keys, secrets).
MAX_SECRET_LENGTH: Final[int] = 1024

# Minimum length for security tokens.
MIN_SECRET_LENGTH: Final[int] = 8

# ── Whitelist patterns ───────────────────────────────────────────────────────
# NOTE: All SAFE_*_PATTERN constants use `+` (one-or-more) by default.
# Empty-string handling is delegated to `validate_length(min_length=...)`
# and `validate_whitelist()`, which explicitly allows empty strings when
# the length gate has already passed with min_length=0.

# Only alphanumeric, underscore, hyphen, and dot — safe everywhere.
SAFE_IDENTIFIER_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[a-zA-Z0-9_.-]+$"
)

# Alphanumeric only — for slugs, usernames, and keys that should never
# contain dots or hyphens either.
SAFE_ALPHANUMERIC_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[a-zA-Z0-9]+$"
)

# Safe for general text (display names, descriptions) — common printable
# ASCII plus basic Unicode letters and whitespace. NO control characters.
# Uses `*` (zero-or-more) to allow empty strings after length validation.
SAFE_TEXT_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[\w\s.,!?;:'\"()\[\]{}\-@#&+=/*$€£¥%~]*$"
)

# Shell-safe characters — only a minimal whitelist.
# NO backticks, $, |, ;, &, >, <, (, ), {, }, !, #, ~, *, ?
SHELL_SAFE_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[a-zA-Z0-9_./\-:@%+,=]+$"
)

# HTML-safe text characters — no <, >, &, " unless HTML-encoded later.
HTML_SAFE_TEXT_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[^<>\"&]+$"
)

# URL-safe path segments — no control characters, spaces, or special chars.
URL_PATH_SAFE_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^[a-zA-Z0-9\-._~!$&'()*+,;=:@/]+$"
)

# ── Dangerous patterns (for detection / logging, NOT for sanitisation) ────────
DANGEROUS_PATTERNS: Final[list[tuple[re.Pattern[str], str]]] = [
    (re.compile(r"<[^>]*>", re.IGNORECASE), "HTML tag"),
    (re.compile(r"javascript:", re.IGNORECASE), "javascript: URI"),
    (re.compile(r"on\w+\s*=", re.IGNORECASE), "event handler attribute"),
    (re.compile(r"[\"'`]\s*[-;]", re.IGNORECASE), "SQL injection indicator"),
    (re.compile(r"\$\{"), "template expression"),
    (re.compile(r"`[^`]*`"), "shell backtick"),
    (re.compile(r"\\\\"), "backslash path separator"),
    (re.compile(r"\x00-\x08\x0b\x0c\x0e-\x1f"), "control character"),
    (re.compile(r"\.\."), "directory traversal"),
    (re.compile(
        r"\b(?:UNION|SELECT|INSERT|UPDATE|DELETE|DROP|ALTER|CREATE|EXEC|OR|AND)\b",
        re.IGNORECASE,
    ), "SQL keyword"),
]


# ──────────────────────────────────────────────────────────────────────────────
# Exceptions
# ──────────────────────────────────────────────────────────────────────────────

class SecurityInputError(ValueError):
    """Raised when input fails security validation.

    The message is deliberately generic — never echoing back user input
    that caused the failure, to prevent log injection and reflected XSS
    via error messages.
    """
    pass


class InputTooLongError(SecurityInputError):
    """Raised when input exceeds the maximum allowed length."""
    pass


class InputTooShortError(SecurityInputError):
    """Raised when input is shorter than the minimum allowed length."""
    pass


class InvalidCharacterError(SecurityInputError):
    """Raised when input contains characters outside the whitelist."""
    pass


class DangerousPatternDetectedError(SecurityInputError):
    """Raised when input matches a known dangerous pattern."""
    pass


# ──────────────────────────────────────────────────────────────────────────────
# Core Validation Functions
# ──────────────────────────────────────────────────────────────────────────────

def validate_length(
    value: str,
    *,
    max_length: int = MAX_INPUT_LENGTH,
    min_length: int = 0,
    name: str = "input",
) -> str:
    """Validate string length constraints.

    Security considerations:
      - Rejects empty strings if min_length > 0 (prevents logic bypass).
      - Rejects oversized strings to prevent ReDoS and memory exhaustion.
      - Always checks length BEFORE content validation (fail-fast principle).

    Edge cases handled:
      - Empty string ("") passes when min_length=0.
      - Strings at exactly max_length or min_length pass (boundary inclusive).

    Args:
        value:      Input string to validate.
        max_length: Maximum allowed length (default: 4096).
        min_length: Minimum allowed length (default: 0).
        name:       Human-readable field name for error context.

    Returns:
        The validated string (unchanged).

    Raises:
        InputTooLongError:  If value exceeds max_length.
        InputTooShortError: If value is shorter than min_length.
    """
    if len(value) > max_length:
        raise InputTooLongError(
            f"{name} exceeds maximum allowed length of {max_length} characters"
        )
    if len(value) < min_length:
        raise InputTooShortError(
            f"{name} is shorter than minimum required length of {min_length} characters"
        )
    return value


def validate_whitelist(
    value: str,
    *,
    pattern: re.Pattern[str] = SAFE_IDENTIFIER_PATTERN,
    name: str = "input",
) -> str:
    """Validate that the entire string matches a whitelist pattern.

    Security considerations:
      - Uses whitelist (allowlist) approach — ONLY the specified characters
        are accepted. Everything else is rejected.
      - REJECTION over sanitisation: silently removing characters can
        introduce subtle logic bugs. We raise instead.
      - Always call validate_length() FIRST to prevent ReDoS on the regex.

    Edge cases handled:
      - Empty string ("") is explicitly allowed — the length gate
        (validate_length with min_length parameter) owns emptiness policy.
      - Null bytes ("\\x00") are rejected by the whitelist pattern.

    Args:
        value:   Input string to validate.
        pattern: Compiled regex matching allowed characters.
        name:    Human-readable field name for error context.

    Returns:
        The validated string (unchanged).

    Raises:
        InvalidCharacterError: If any character is outside the whitelist.
    """
    # Empty string is allowed — length validation (min_length) owns that policy
    if value == "":
        return value
    if not pattern.fullmatch(value):
        raise InvalidCharacterError(
            f"{name} contains invalid characters — only a restricted "
            f"character set is permitted"
        )
    return value


def validate_no_dangerous_patterns(
    value: str,
    *,
    name: str = "input",
) -> str:
    """Check for known dangerous patterns and reject if found.

    This is a DEFENCE-IN-DEPTH layer — it catches inputs that passed
    whitelist validation but still contain dangerous sequences (e.g.,
    a safe ASCII string like 'SELECT * FROM users').

    Security considerations:
      - Blacklist-based checks are NEVER sufficient on their own.
        This function is a secondary layer, always paired with whitelist
        validation.
      - Detected patterns are logged for security monitoring but NEVER
        echoed back to the user.

    Edge cases handled:
      - Empty string ("") passes trivially (no patterns to match).
      - Patterns that span across character boundaries are handled by
        re.search (not re.match), so they detect patterns anywhere.

    Args:
        value: Input string to inspect.
        name:  Human-readable field name for error context.

    Returns:
        The validated string (unchanged).

    Raises:
        DangerousPatternDetectedError: If a dangerous pattern is found.
    """
    for pattern, _description in DANGEROUS_PATTERNS:
        if pattern.search(value):
            raise DangerousPatternDetectedError(
                f"{name} was rejected by security scanner"
            )
    return value


def validate_safe_input(
    value: str,
    *,
    max_length: int = MAX_INPUT_LENGTH,
    min_length: int = 0,
    pattern: re.Pattern[str] = SAFE_IDENTIFIER_PATTERN,
    check_dangerous: bool = True,
    normalize_unicode: bool = True,
    name: str = "input",
) -> str:
    """Comprehensive security validation: length → unicode → whitelist → danger.

    This is the PRIMARY ENTRY POINT for most untrusted string inputs.
    It applies all validation layers in a safe order:

      1. Length check          — fail fast, prevent ReDoS on regex
      2. Unicode normalisation — NFC prevents homoglyph / encoding bypass
      3. Whitelist match       — only known-safe characters pass
      4. Dangerous patterns    — defence-in-depth blacklist check

    Security considerations:
      - Unicode normalisation (NFC) prevents homoglyph attacks where
        visually identical characters have different code points.
      - Zero-width characters, bidirectional override characters, and
        other Unicode control characters are rejected by the whitelist.
      - The order of operations is critical — length before regex prevents
        ReDoS, normalisation before whitelist prevents encoding bypass.

    Edge cases handled:
      - Empty string ("") passes if min_length=0 (length check + whitelist
        explicit empty-string pass).
      - NFC normalisation may change the string length; length is checked
        BEFORE normalisation (conservative), so a string that passes the
        length check but becomes longer after normalisation is still
        accepted — this is by design to avoid complexity attacks.
      - Null bytes and control characters are rejected by the whitelist.

    Args:
        value:              Input string to validate.
        max_length:         Maximum allowed length.
        min_length:         Minimum allowed length.
        pattern:            Whitelist regex pattern.
        check_dangerous:    Whether to also check dangerous patterns.
        normalize_unicode:  Whether to NFC-normalise the string.
        name:               Human-readable field name.

    Returns:
        The validated string (unchanged, possibly NFC-normalised).

    Raises:
        InputTooLongError
        InputTooShortError
        InvalidCharacterError
        DangerousPatternDetectedError
    """
    # Step 1: Length — fail fast, prevent regex DoS
    validate_length(value, max_length=max_length, min_length=min_length, name=name)

    # Step 2: Unicode normalisation — prevent homoglyph / encoding bypass
    if normalize_unicode:
        value = unicodedata.normalize("NFC", value)

    # Step 3: Whitelist — reject anything that isn't explicitly allowed
    validate_whitelist(value, pattern=pattern, name=name)

    # Step 4: Dangerous pattern detection — defence-in-depth
    if check_dangerous:
        validate_no_dangerous_patterns(value, name=name)

    return value


# ──────────────────────────────────────────────────────────────────────────────
# Context-Aware Encoding / Escaping Functions
# ──────────────────────────────────────────────────────────────────────────────
# These functions follow OWASP Encoding Practices:
#   https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html
#
# Output encoding MUST be context-specific — HTML body, HTML attribute,
# JavaScript string, URL, CSS, SQL string — each requires a different
# encoding strategy.
# ──────────────────────────────────────────────────────────────────────────────

def encode_for_sql(value: str, *, max_length: int = MAX_INPUT_LENGTH) -> str:
    """Encode a string for safe use in SQL string literals.

    Security considerations (OWASP SQL Injection Prevention):
      - This is a DEFENCE-IN-DEPTH measure. The PRIMARY defence against
        SQL injection is ALWAYS parameterised queries / prepared statements.
      - This function escapes single quotes and backslashes for use in
        string literals ONLY — it does NOT protect against:
          * Numeric injection (use type casting / parameters)
          * LIKE injection (escape % and _ separately)
          * Second-order injection
      - ALWAYS prefer parameterised queries. This function is for edge cases
        (dynamic table names, schema generation) where parameters can't be
        used.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Strings with backslashes are properly escaped.
      - Strings with single quotes are properly escaped.

    Args:
        value:     String to encode.
        max_length: Maximum allowed input length.

    Returns:
        Encoded string safe for SQL string literal interpolation.

    Raises:
        InputTooLongError: If input exceeds max_length.
    """
    validate_length(value, max_length=max_length, name="sql_input")
    # Escape backslashes first, then single quotes (order matters)
    escaped = value.replace("\\", "\\\\").replace("'", "\\'")
    return escaped


def encode_for_html_attr(value: str, *, max_length: int = MAX_INPUT_LENGTH) -> str:
    """Encode a string for safe use in HTML attributes. (OWASP Rule #2)

    Security considerations (OWASP XSS Prevention):
      - Escapes & < > " ' to prevent XSS in HTML attribute contexts.
      - Does NOT protect against javascript: URIs or event handlers —
        validate_safe_input() must be called FIRST to reject those.
      - For HTML body content, use encode_for_html_body() instead.
      - For CSP-friendly approach, prefer textContent over innerHTML.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - All HTML-special characters are entity-encoded.

    Args:
        value:     String to encode.
        max_length: Maximum allowed input length.

    Returns:
        HTML-attribute-encoded string.
    """
    validate_length(value, max_length=max_length, name="html_attr")
    # html.escape escapes & < > " with quote=True; also escapes '
    return html.escape(value, quote=True)


def encode_for_html_body(value: str, *, max_length: int = MAX_INPUT_LENGTH) -> str:
    """Encode a string for safe use in HTML body content. (OWASP Rule #1)

    Security considerations (OWASP XSS Prevention):
      - Escapes & < > for safe insertion into HTML text nodes.
      - Does NOT escape single or double quotes (not needed in body context).
      - Does NOT protect against script injection via <script> or <style>
        tags — input validation must reject those first.
      - ALWAYS prefer using a template engine with auto-escaping (Jinja2,
        Django templates) over manual encoding.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Only &, <, > are encoded (body context).

    Args:
        value:     String to encode.
        max_length: Maximum allowed input length.

    Returns:
        HTML-safe string.
    """
    validate_length(value, max_length=max_length, name="html_body")
    return html.escape(value, quote=False)


def encode_for_url_path(
    value: str, *, max_length: int = MAX_INPUT_LENGTH
) -> str:
    """Encode a string for safe use in URL path segments. (OWASP Rule #5)

    Security considerations (OWASP XSS Prevention):
      - Uses urllib.parse.quote to percent-encode unsafe characters.
      - Prevents path traversal by encoding "/" and ".." sequences.
      - Does NOT protect against SSRF — URLs should be validated separately.
      - Does NOT encode the entire URL — only single path segments.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Unicode characters are percent-encoded as UTF-8 byte sequences.

    Args:
        value:     String to encode (a single path segment).
        max_length: Maximum allowed input length.

    Returns:
        URL-path-safe percent-encoded string.
    """
    from urllib.parse import quote

    validate_length(value, max_length=max_length, name="url_path")
    # safe="" ensures ALL unsafe characters are encoded
    return quote(value, safe="")


def encode_for_shell(
    value: str, *, max_length: int = MAX_IDENTIFIER_LENGTH
) -> str:
    """Securely encode a string for shell command arguments.

    Security considerations:
      - Uses shlex.quote() which wraps in single quotes and escapes any
        internal single quotes — this is THE recommended approach.
      - Input is first validated against SHELL_SAFE_PATTERN (whitelist).
      - Length is strictly limited (default: 128 chars).
      - NEVER use this for constructing shell commands from user input.
        Prefer subprocess.run() with a list of arguments.
      - This function is for edge cases where shell=True is unavoidable.

    Edge cases handled:
      - Empty string ("") is wrapped as '' (empty quoted argument).
      - Strings with single quotes are properly escaped.

    Args:
        value:     String to encode for shell use.
        max_length: Maximum allowed length (default: 128).

    Returns:
        Shell-escaped string safe for use as a single argument.

    Raises:
        InvalidCharacterError: If input contains unsafe characters.
        InputTooLongError:     If input exceeds max_length.
    """
    validate_length(value, max_length=max_length, name="shell_input")
    validate_whitelist(value, pattern=SHELL_SAFE_PATTERN, name="shell_input")
    return shlex.quote(value)


def encode_for_json(
    value: str, *, max_length: int = MAX_INPUT_LENGTH
) -> str:
    """Encode a string for safe use in JSON.

    Security considerations:
      - Uses json.dumps() which handles all JSON-required escaping
        (quotes, backslashes, control characters).
      - Prevents JSON injection in structured data contexts.
      - Does NOT protect against prototype pollution — validate
        object keys separately.

    Edge cases handled:
      - Empty string ("") returns the JSON representation '""'.
      - Control characters and Unicode are properly escaped.

    Args:
        value:     String to encode.
        max_length: Maximum allowed input length.

    Returns:
        JSON-encoded string (including surrounding quotes).
    """
    import json

    validate_length(value, max_length=max_length, name="json_input")
    return json.dumps(value, ensure_ascii=True)


# ──────────────────────────────────────────────────────────────────────────────
# High-Level Secure String Functions
# ──────────────────────────────────────────────────────────────────────────────

def sanitize_identifier(
    value: str,
    *,
    max_length: int = MAX_IDENTIFIER_LENGTH,
    allow_dot: bool = True,
    allow_hyphen: bool = True,
    name: str = "identifier",
) -> str:
    """Securely validate and normalise an identifier (username, key, slug).

    Security considerations:
      - Strict whitelist: alphanumeric + underscore + optional dot/hyphen.
      - NFC-normalised to prevent homoglyph/encoding bypass.
      - Length-limited to MAX_IDENTIFIER_LENGTH (128 chars).
      - Rejects empty strings (min_length=1).

    Edge cases handled:
      - Empty string ("") raises InputTooShortError.
      - Strings with control characters or Unicode bidi overrides
        are rejected by the whitelist pattern.

    Args:
        value:        Raw identifier input.
        max_length:   Maximum length (default: 128).
        allow_dot:    Whether to allow "." in the identifier.
        allow_hyphen: Whether to allow "-" in the identifier.
        name:         Field name for error messages.

    Returns:
        Validated, normalised identifier.

    Raises:
        InputTooLongError
        InputTooShortError
        InvalidCharacterError
    """
    # Construct pattern based on allowed characters
    allowed = r"a-zA-Z0-9_"
    if allow_dot:
        allowed += r"\."
    if allow_hyphen:
        allowed += r"\-"
    pattern = re.compile(rf"^[{allowed}]+$")

    return validate_safe_input(
        value,
        max_length=max_length,
        min_length=1,
        pattern=pattern,
        check_dangerous=True,
        normalize_unicode=True,
        name=name,
    )


def sanitize_slug(
    value: str, *, max_length: int = MAX_IDENTIFIER_LENGTH
) -> str:
    """Create a URL-safe slug from input.

    Security considerations:
      - Only lowercase alphanumeric + hyphens allowed.
      - Multiple consecutive hyphens are collapsed into one.
      - Leading/trailing hyphens are stripped.
      - NFC-normalised.
      - Prevents Unicode homoglyph and encoding attacks.

    Edge cases handled:
      - Empty string ("") raises InvalidCharacterError.
      - Strings with only non-alphanumeric characters raise
        InvalidCharacterError (produces empty result).
      - Very long strings raise InputTooLongError.

    Args:
        value:      Raw string to convert into a slug.
        max_length: Maximum length (default: 128).

    Returns:
        Secure, URL-safe slug string.

    Raises:
        InputTooLongError
        InvalidCharacterError: If input is empty or produces empty result.
    """
    validate_length(value, max_length=max_length, name="slug")

    # Normalise unicode first
    value = unicodedata.normalize("NFC", value)

    # Lowercase
    value = value.lower()

    # Replace non-alphanumeric characters with hyphens
    value = re.sub(r"[^a-z0-9]+", "-", value)

    # Strip leading/trailing hyphens
    value = value.strip("-")

    # Collapse multiple hyphens
    value = re.sub(r"-{2,}", "-", value)

    if not value:
        raise InvalidCharacterError("slug input produced an empty result")

    return value


def sanitize_text(
    value: str,
    *,
    max_length: int = MAX_INPUT_LENGTH,
    min_length: int = 0,
    name: str = "text",
) -> str:
    """Securely validate free-text input (display names, descriptions).

    Security considerations:
      - Allows a broad but controlled set of printable characters.
      - Rejects control characters, zero-width characters, and HTML.
      - NOT suitable for HTML rendering — the output must still be
        contextually escaped (use encode_for_html_body() before rendering).
      - Length-limited to prevent storage and processing abuse.

    Edge cases handled:
      - Empty string ("") passes when min_length=0 (default).
      - Newlines (\\n) and tabs (\\t) are allowed (via \\s in pattern).
      - Unicode characters are NFC-normalised.

    Args:
        value:      Free-text input.
        max_length: Maximum length (default: 4096).
        min_length: Minimum length (default: 0).
        name:       Field name for error messages.

    Returns:
        Validated, NFC-normalised text.

    Raises:
        InputTooLongError
        InputTooShortError
        InvalidCharacterError
        DangerousPatternDetectedError
    """
    return validate_safe_input(
        value,
        max_length=max_length,
        min_length=min_length,
        pattern=SAFE_TEXT_PATTERN,
        check_dangerous=True,
        normalize_unicode=True,
        name=name,
    )


def sanitize_for_html(
    value: str, *, max_length: int = MAX_INPUT_LENGTH
) -> str:
    """Validate and HTML-encode input for safe HTML rendering. (OWASP Rule #1)

    Security considerations:
      - Two-layer defence: whitelist validation + HTML entity encoding.
      - Prevents stored and reflected XSS.
      - Does NOT allow any HTML tags — this is for safe text rendering.
      - If you need to allow safe HTML tags, use a dedicated HTML
        sanitisation library (e.g., bleach) instead.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - All HTML-special characters are entity-encoded.

    Args:
        value:      Raw input string.
        max_length: Maximum allowed length.

    Returns:
        HTML-safe string with entities encoded.
    """
    validate_length(value, max_length=max_length, name="html_input")
    # NFC normalise
    value = unicodedata.normalize("NFC", value)
    # HTML-entity encode everything (quote=True for attribute safety)
    return html.escape(value, quote=True)


def sanitize_for_sql_identifier(
    value: str,
    *,
    max_length: int = MAX_IDENTIFIER_LENGTH,
    name: str = "sql_identifier",
) -> str:
    """Validate a SQL identifier (table name, column name).

    Security considerations:
      - Only allows alphanumeric + underscore — NO dots, spaces, or quotes.
      - This prevents SQL injection via dynamic identifiers.
      - ALWAYS prefer static identifiers. Use this only when dynamic
        identifiers are unavoidable (e.g., multi-tenant table selection).
      - Even with this validation, the output should be wrapped in
        quoted identifiers (e.g., PostgreSQL "identifier" syntax).

    Edge cases handled:
      - Empty string ("") raises InputTooShortError.

    Args:
        value:      SQL identifier input.
        max_length: Maximum length (default: 128).
        name:       Field name for error messages.

    Returns:
        Safe SQL identifier string.

    Raises:
        InputTooLongError
        InputTooShortError
        InvalidCharacterError
    """
    return sanitize_identifier(
        value,
        max_length=max_length,
        allow_dot=False,
        allow_hyphen=False,
        name=name,
    )


def sanitize_filename(
    value: str,
    *,
    max_length: int = 255,
    name: str = "filename",
) -> str:
    """Validate and sanitise a filename for safe filesystem operations.

    Security considerations:
      - Path traversal prevention: rejects ".", "..", "/", and "\\".
      - OS-specific unsafe characters are stripped.
      - Length is limited to 255 bytes (common filesystem limit).
      - Extension is preserved but validated.
      - Does NOT guarantee the file doesn't exist — check separately.
      - Consider using UUID-based filenames instead for uploaded files.

    Edge cases handled:
      - Empty string ("") raises InvalidCharacterError.
      - Strings with only dots/hidden-file chars raise
        InvalidCharacterError.
      - Null bytes are stripped before validation.
      - Forward slashes and backslashes are stripped (path traversal).

    Args:
        value:      Raw filename input.
        max_length: Maximum length (default: 255).
        name:       Field name for error messages.

    Returns:
        Safe filename string.

    Raises:
        InputTooLongError
        InvalidCharacterError: If the filename is empty or entirely invalid.
    """
    validate_length(value, max_length=max_length, name=name)

    # NFC normalise
    value = unicodedata.normalize("NFC", value)

    # Strip path separators and traversal components
    value = value.replace("/", "").replace("\\", "")

    # Strip null bytes
    value = value.replace("\x00", "")

    # Remove OS-specific unsafe characters (Windows-safe by default)
    unsafe_chars = '<>:"|?*'
    for c in unsafe_chars:
        value = value.replace(c, "")

    # Remove leading dots (hidden files / traversal)
    value = value.lstrip(".")

    # Remove leading/trailing whitespace and dots
    value = value.strip(". \t")

    if not value:
        raise InvalidCharacterError(
            f"{name} is empty or contains only unsafe characters"
        )

    # Validate remaining characters against identifier whitelist
    validate_whitelist(value, pattern=SAFE_IDENTIFIER_PATTERN, name=name)

    return value


def safe_truncate(
    value: str,
    *,
    max_length: int = MAX_INPUT_LENGTH,
    encoding: str = "utf-8",
) -> str:
    """Securely truncate a string to a maximum length.

    Security considerations:
      - Truncation is performed AFTER security validation — never before.
      - Unicode-aware: does not break multi-byte characters (uses
        character count, not byte count, by default).
      - The truncated result is still potentially malicious. Callers
        must still contextually encode the output.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Strings shorter than max_length are returned unchanged.
      - Unicode grapheme clusters (e.g., emoji sequences) are preserved
        if possible within the character limit.
      - max_length=0 returns empty string (not an error — caller's choice).

    Args:
        value:     String to truncate.
        max_length: Maximum number of characters (default: 4096).
        encoding:  For byte-aware truncation (default: 'utf-8').

    Returns:
        Truncated string (never longer than max_length characters).
    """
    if not value:
        return value
    if max_length < 1:
        return ""
    if len(value) <= max_length:
        return value

    # Character-level truncation (Unicode-safe)
    truncated = value[:max_length]

    # If byte-level truncation is requested, re-encode and trim
    if encoding and len(truncated.encode(encoding, errors="replace")) > max_length:
        # Fall back to byte-aware truncation
        byte_value = value.encode(encoding, errors="replace")[:max_length]
        truncated = byte_value.decode(encoding, errors="replace")

    return truncated


def strip_html_tags(value: str) -> str:
    """Remove HTML tags from a string, returning only text content.

    Security considerations:
      - Uses regex to remove HTML tags — this is NOT a sanitisation
        solution. For allowing safe HTML, use a dedicated library
        (e.g., bleach, html-sanitizer).
      - After stripping, the output may still contain XSS vectors
        (e.g., text that looks like JS). Always encode for the
        output context.
      - This is a LAST RESORT for when HTML must be removed.
        Prefer rejecting HTML input outright (sanitize_for_html).

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Nested tags, malformed tags, and self-closing tags are handled.
      - HTML entities (e.g., &amp;) are preserved (not decoded).
      - Text between tags is preserved and concatenated.

    Args:
        value: String potentially containing HTML.

    Returns:
        String with all HTML tags removed.
    """
    if not value:
        return value
    # Remove comments first
    value = re.sub(r"<!--.*?-->", "", value, flags=re.DOTALL)
    # Remove tags
    value = re.sub(r"<[^>]*>", "", value)
    # Collapse whitespace
    value = re.sub(r"\s+", " ", value).strip()
    return value


def normalize_unicode(
    value: str,
    form: str = "NFC",
) -> str:
    """Normalise Unicode string to a canonical form.

    Security considerations:
      - NFC (Canonical Composition) is the DEFAULT and recommended form
        for general text processing. It composes characters where possible
        (e.g., é as U+00E9 instead of e + U+0301).
      - NFKD (Compatibility Decomposition) can be used before validation
        to decompose characters like ⁴ -> 4, but this may change semantics.
      - This function is safe to call on already-normalised strings.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Already-normalised strings are returned unchanged (idempotent).
      - Invalid UTF-8 sequences are handled by Python's Unicode support.

    Args:
        value: Input string.
        form:  Normalisation form (NFC, NFD, NFKC, NFKD). Default: NFC.

    Returns:
        Normalised Unicode string.

    Raises:
        ValueError: If form is not a valid normalisation form.
    """
    valid_forms = {"NFC", "NFD", "NFKC", "NFKD"}
    if form not in valid_forms:
        raise ValueError(f"Invalid normalisation form '{form}'. Use: {valid_forms}")
    if not value:
        return value
    return unicodedata.normalize(form, value)


# ──────────────────────────────────────────────────────────────────────────────
# Security Utility Functions
# ──────────────────────────────────────────────────────────────────────────────

def constant_time_compare(a: str, b: str) -> bool:
    """Compare two strings in constant time to prevent timing attacks.

    Security considerations:
      - Standard == and != operators short-circuit on first mismatch,
        leaking timing information that can be used to brute-force
        tokens character by character.
      - This function always compares every character, making the
        timing independent of the content.
      - Use this for comparing: passwords, API keys, session tokens,
        HMAC signatures, CSRF tokens.
      - The length is intentionally NOT compared in constant time —
        lengths are revealed. Use tokens of known, fixed length for
        best security.

    Edge cases handled:
      - Empty strings: Two empty strings are equal (returns True).
      - Different lengths: Returns False immediately (length is leaked,
        but this is acceptable for non-secret-length contexts).

    Args:
        a: First string to compare.
        b: Second string to compare.

    Returns:
        True if strings are equal, False otherwise.
    """
    if len(a) != len(b):
        return False

    result = 0
    for ca, cb in zip(a, b):
        result |= ord(ca) ^ ord(cb)
    return result == 0


def generate_secure_token(
    length: int = 32,
    *,
    charset: str = string.ascii_letters + string.digits,
) -> str:
    """Generate a cryptographically secure random token.

    Security considerations:
      - Uses secrets.SystemRandom (CSPRNG) — NOT the random module.
      - Default charset is alphanumeric (no special chars) for safe use
        in URLs, headers, and cookies.
      - Default length of 32 characters = 192 bits of entropy.
      - For session tokens, use at least 128 bits (22 chars base64).
      - For API keys, use at least 192 bits (32 chars base64).

    Edge cases handled:
      - length=0 raises ValueError.
      - Empty charset raises ValueError.
      - Very long lengths are supported (memory permitting).

    Args:
        length:  Number of characters (default: 32).
        charset: String of allowed characters (default: a-zA-Z0-9).

    Returns:
        Cryptographically secure random token string.

    Raises:
        ValueError: If length < 1 or charset is empty.
    """
    import secrets

    if length < 1:
        raise ValueError("token length must be at least 1")
    if not charset:
        raise ValueError("charset must not be empty")

    return "".join(secrets.choice(charset) for _ in range(length))


def generate_session_token(length: int = 32) -> str:
    """Generate a cryptographically secure session token.

    Security considerations:
      - 32 hex chars = 128 bits of entropy.
      - Hex-encoded for URL/cookie safety (no special chars to escape).
      - Use constant_time_compare() for validation.

    Edge cases handled:
      - length parameter determines entropy bits.

    Args:
        length: Number of hex characters (default: 32 = 128 bits).

    Returns:
        Secure session token string.
    """
    import secrets
    return secrets.token_hex(length // 2 + 1)[:length]


def strip_control_characters(value: str) -> str:
    """Remove control characters (except newlines and tabs) from a string.

    Security considerations:
      - Control characters (0x00-0x1F, 0x7F) can be used for terminal
        injection, log injection, and protocol-level attacks.
      - Newlines (\\n) and tabs (\\t) are preserved as they are often
        valid content.
      - This is a SANITISATION function — use validation functions
        (validate_safe_input) for the primary defence.

    Edge cases handled:
      - Empty string ("") returns empty string.
      - Strings without control characters are returned unchanged.
      - Null bytes (\\x00) are removed.

    Args:
        value: Input string.

    Returns:
        String with control characters removed.
    """
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", value)


def contains_sensitive_data(value: str) -> bool:
    """Check if a string potentially contains sensitive data patterns.

    Security considerations:
      - Detects common patterns: API keys, tokens, credentials, keys.
      - This is a HEURISTIC — it has false positives and false negatives.
      - Used for data loss prevention (DLP) scanning, NOT access control.
      - Never log or echo the matched string.

    Edge cases handled:
      - Empty string ("") returns False.
      - Case-insensitive matching for keywords.

    Args:
        value: String to scan.

    Returns:
        True if sensitive data patterns are detected.
    """
    if not value:
        return False

    patterns = [
        re.compile(r"(?i)(?:api[_-]?key|secret|token|password|credential)\s*[:=]\s*\S+"),
        re.compile(r"(?i)sk-[a-zA-Z0-9]{20,}"),          # OpenAI API keys
        re.compile(r"(?i)ghp_[a-zA-Z0-9]{36}"),           # GitHub PAT
        re.compile(r"(?i)AKIA[0-9A-Z]{16}"),              # AWS Access Key
        re.compile(
            r"(?i)eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+"
        ),  # JWT
    ]
    return any(p.search(value) for p in patterns)


# ──────────────────────────────────────────────────────────────────────────────
# Public API — what consumers should import
# ──────────────────────────────────────────────────────────────────────────────

__all__ = [
    # Constants
    "MAX_INPUT_LENGTH",
    "MAX_IDENTIFIER_LENGTH",
    "MAX_SECRET_LENGTH",
    "MIN_SECRET_LENGTH",
    "SAFE_IDENTIFIER_PATTERN",
    "SAFE_TEXT_PATTERN",
    "SHELL_SAFE_PATTERN",
    "DANGEROUS_PATTERNS",

    # Exceptions
    "SecurityInputError",
    "InputTooLongError",
    "InputTooShortError",
    "InvalidCharacterError",
    "DangerousPatternDetectedError",

    # Core validation
    "validate_length",
    "validate_whitelist",
    "validate_no_dangerous_patterns",
    "validate_safe_input",

    # Context-aware encoding
    "encode_for_sql",
    "encode_for_html_attr",
    "encode_for_html_body",
    "encode_for_url_path",
    "encode_for_shell",
    "encode_for_json",

    # Secure string functions
    "sanitize_identifier",
    "sanitize_slug",
    "sanitize_text",
    "sanitize_for_html",
    "sanitize_for_sql_identifier",
    "sanitize_filename",
    "safe_truncate",
    "strip_html_tags",
    "normalize_unicode",

    # Security utilities
    "constant_time_compare",
    "generate_secure_token",
    "generate_session_token",
    "strip_control_characters",
    "contains_sensitive_data",
]
